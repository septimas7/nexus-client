//! CMP-004: signed self-update.
//!
//! The public key is compiled in from the `NEXUS_DESKTOP_UPDATER_PUBKEY` build
//! variable that the release workflow sets. A build without it is a local or
//! unsigned build, and it does not check for updates at all rather than trust an
//! artifact it cannot verify: the tray says so and no request is made.

use std::time::Duration;

use chrono::Utc;
use nexus_desktop_core::update;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

use crate::{AppState, tray, window};

/// The updater's public key, baked at build time. `None` disables the updater;
/// an empty or blank variable counts as `None` (`update::configured_pubkey`).
pub const PUBKEY: Option<&str> =
    update::configured_pubkey(option_env!("NEXUS_DESKTOP_UPDATER_PUBKEY"));

/// How often the schedule is consulted. The answer is usually "not yet"; the
/// short tick exists so a laptop that slept through its six hour mark checks
/// soon after it wakes instead of six hours later.
const SCHEDULE_TICK: Duration = Duration::from_secs(15 * 60);

/// The path a Nexus instance would serve its own update manifest from. Nothing
/// serves it today; the updater asks for it first so that when a platform route
/// appears, installed clients pick it up with no client release
/// (Technical Design, CMP-004 decision C).
const INSTANCE_MANIFEST_PATH: &str = "desktop/latest.json";

/// Whether the updater plugin can be registered, with the reason when it
/// cannot.
///
/// Two things have to agree: a public key compiled in from the build variable,
/// and the `plugins.updater` section of the configuration, which the release
/// workflow writes in the same step that exports the key. The plugin reads its
/// endpoints from that section and refuses to initialise without it, so a build
/// with one and not the other is treated as an unsigned build rather than
/// registered and left to fail during startup, which is how v0.1.6 died.
pub fn registration(config: &tauri::Config) -> Result<&'static str, &'static str> {
    let Some(pubkey) = PUBKEY else {
        return Err("this build carries no updater public key");
    };
    if !config.plugins.0.contains_key("updater") {
        return Err("this build carries an updater public key but no updater configuration");
    }
    Ok(pubkey)
}

/// Whether this running app can update itself.
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.state::<AppState>().updater_enabled
}

/// What the updating page renders.
#[derive(Clone, Serialize)]
struct Progress {
    version: String,
    downloaded: u64,
    total: Option<u64>,
    phase: &'static str,
    reason: Option<String>,
}

/// Start the background schedule: once shortly after launch, then whenever
/// [`update::schedule`] says a check is due.
pub fn spawn_schedule<R: Runtime>(app: AppHandle<R>) {
    if !is_enabled(&app) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(update::LAUNCH_DELAY).await;
        loop {
            let last_check = {
                let state = app.state::<AppState>();
                let config = state.config.lock().expect("configuration lock");
                config.last_update_check
            };
            if update::schedule(Utc::now(), last_check) {
                check_and_prompt(app.clone(), false).await;
            }
            tokio::time::sleep(SCHEDULE_TICK).await;
        }
    });
}

/// Look for a newer release and, if there is one, offer it.
///
/// `interactive` is true for the tray's "Check for updates" item, which is the
/// only case where "you are up to date" is worth a dialog.
pub async fn check_and_prompt<R: Runtime>(app: AppHandle<R>, interactive: bool) {
    if !is_enabled(&app) {
        if interactive {
            notify(
                &app,
                "Updates",
                "This build cannot update itself. Install a signed release to get updates.",
            );
        }
        return;
    }

    let updater = match build_updater(&app) {
        Ok(updater) => updater,
        Err(reason) => {
            log::error!("the updater could not be prepared: {reason}");
            if interactive {
                notify(&app, "Update available", &failure_copy(&reason));
            }
            return;
        }
    };

    let result = updater.check().await;
    record_check(&app);

    let update = match result {
        Ok(Some(update)) => update,
        Ok(None) => {
            clear_pending(&app);
            if interactive {
                notify(&app, "Nexus Desktop", "Nexus Desktop is up to date.");
            }
            return;
        }
        Err(error) => {
            log::warn!("the update check failed: {error}");
            if interactive {
                notify(&app, "Update", &failure_copy(&error.to_string()));
            }
            return;
        }
    };

    let version = update.version.clone();
    log::info!("Nexus Desktop v{version} is available");
    {
        let state = app.state::<AppState>();
        *state.pending_update.lock().expect("pending update lock") = Some(version.clone());
    }
    tray::refresh(&app);

    let accepted = app
        .dialog()
        .message(format!(
            "Nexus Desktop v{version} is available. Install now? The app restarts after installing."
        ))
        .title("Update available")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Install now".to_string(),
            "Later".to_string(),
        ))
        .blocking_show();

    if !accepted {
        log::info!("the update to v{version} was deferred");
        return;
    }

    install(app, update, version).await;
}

async fn install<R: Runtime>(
    app: AppHandle<R>,
    update: tauri_plugin_updater::Update,
    version: String,
) {
    window::show(&app);
    window::show_local_page(&app, window::UPDATING_PAGE);
    emit(&app, &version, 0, None, "downloading", None);

    let progress_app = app.clone();
    let progress_version = version.clone();
    let mut downloaded: u64 = 0;

    let finish_app = app.clone();
    let finish_version = version.clone();

    let outcome = update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                emit(
                    &progress_app,
                    &progress_version,
                    downloaded,
                    total,
                    "downloading",
                    None,
                );
            },
            move || {
                emit(&finish_app, &finish_version, 0, None, "installing", None);
            },
        )
        .await;

    match outcome {
        Ok(()) => {
            log::info!("Nexus Desktop v{version} installed; restarting");
            app.restart();
        }
        Err(error) => {
            let reason = error.to_string();
            log::error!("the update to v{version} failed: {reason}");
            emit(
                &app,
                &version,
                0,
                None,
                "failed",
                Some(failure_copy(&reason)),
            );
            notify(&app, "Update", &failure_copy(&reason));
        }
    }
}

/// Build the updater for one check, asking the bound instance first.
fn build_updater<R: Runtime>(app: &AppHandle<R>) -> Result<tauri_plugin_updater::Updater, String> {
    let builder = app.updater_builder();

    let instance_endpoint = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        config
            .instance_url
            .as_ref()
            // Only over https: the updater refuses plaintext endpoints, and an
            // unverified manifest over a plain http LAN link is not a source
            // this client will take an executable from.
            .filter(|url| url.scheme() == "https")
            .and_then(|url| url.join(INSTANCE_MANIFEST_PATH).ok())
    };

    let builder = match instance_endpoint {
        Some(first) => {
            let mut endpoints = vec![first];
            endpoints.extend(configured_endpoints(app));
            match builder.endpoints(endpoints) {
                Ok(builder) => builder,
                Err(error) => {
                    log::warn!("keeping the configured update endpoints: {error}");
                    app.updater_builder()
                }
            }
        }
        None => builder,
    };

    builder.build().map_err(|error| error.to_string())
}

/// The endpoints from `tauri.conf.json`, so the public releases URL is written
/// in exactly one place.
fn configured_endpoints<R: Runtime>(app: &AppHandle<R>) -> Vec<Url> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|plugin| plugin.get("endpoints"))
        .and_then(|endpoints| endpoints.as_array())
        .map(|endpoints| {
            endpoints
                .iter()
                .filter_map(|endpoint| endpoint.as_str())
                .filter_map(|endpoint| Url::parse(endpoint).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn record_check<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let mut config = state.config.lock().expect("configuration lock");
    config.last_update_check = Some(Utc::now());
    if let Err(reason) = config.save(app) {
        log::error!("could not record the update check: {reason}");
    }
}

fn clear_pending<R: Runtime>(app: &AppHandle<R>) {
    let had_pending = {
        let state = app.state::<AppState>();
        let mut pending = state.pending_update.lock().expect("pending update lock");
        pending.take().is_some()
    };
    if had_pending {
        tray::refresh(app);
    }
}

fn emit<R: Runtime>(
    app: &AppHandle<R>,
    version: &str,
    downloaded: u64,
    total: Option<u64>,
    phase: &'static str,
    reason: Option<String>,
) {
    let payload = Progress {
        version: version.to_string(),
        downloaded,
        total,
        phase,
        reason,
    };
    if let Err(error) = app.emit("nexus://update-progress", payload) {
        log::warn!("could not report update progress: {error}");
    }
}

fn failure_copy(reason: &str) -> String {
    format!("The update could not be installed: {reason}. Try again later.")
}

fn notify<R: Runtime>(app: &AppHandle<R>, title: &str, message: &str) {
    app.dialog().message(message).title(title).show(|_| {});
}
