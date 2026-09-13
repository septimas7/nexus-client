//! Nexus Desktop: a native window around a hosted Nexus instance.
//!
//! CMP-001 (`desktop.shell`) and CMP-004 (`desktop.updater`) of the
//! `client:desktop` unit. The client holds no domain semantics: the window's
//! document is the portal the instance serves, at the instance's own origin, so
//! a feature the platform ships arrives here with no client release. What lives
//! in this crate is only the chrome a browser cannot provide, and every rule it
//! applies is decided in `nexus-desktop-core`, where it can be tested.

mod commands;
mod config;
mod crash;
mod instance;
mod tray;
mod updater;
mod window;

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{Target, TargetKind};

use crate::config::Config;
use crate::instance::InstanceStatus;

/// The launch argument the autostart registration carries, so a login start
/// comes up in the tray instead of opening a window.
const MINIMIZED_ARG: &str = "--minimized";

/// One log file, rotated at 2 MB, with the previous one kept.
const MAX_LOG_BYTES: u128 = 2 * 1024 * 1024;

/// Everything the shell knows at runtime.
#[derive(Default)]
pub struct AppState {
    /// The persisted configuration, mirrored in memory.
    pub config: Mutex<Config>,
    /// What the last probe or page load said about the instance.
    pub instance: Mutex<InstanceStatus>,
    /// The version of an update that has been found and not yet installed.
    pub pending_update: Mutex<Option<String>>,
    /// Generation counter for the page load watchdog in `window.rs`.
    pub load_generation: AtomicU64,
    /// The generation of a page load on the instance origin that has started
    /// and not finished.
    pub pending_load: Mutex<Option<u64>>,
    /// Whether the updater plugin was registered (`updater::registration`).
    pub updater_enabled: bool,
}

/// Build the app and run it. `main.rs` is one line; this is the real entry
/// point, which is also what makes the shell testable as a library.
pub fn run() {
    let context = tauri::generate_context!();
    crash::install(context.config());

    // The updater is registered only when the key and the configuration agree;
    // a build with one and not the other is an unsigned build (CMP-004
    // decision A, and the v0.1.6 startup failure).
    let registration = updater::registration(context.config());

    let mut builder = tauri::Builder::default()
        // Registered first, as the plugin's own documentation requires: a
        // second launch hands its arguments to the running process and focuses
        // the window instead of starting a second tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(Target::new(TargetKind::LogDir {
                    file_name: Some("nexus-desktop".into()),
                }))
                .target(Target::new(TargetKind::Stderr))
                .max_file_size(MAX_LOG_BYTES)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ));

    if let Ok(pubkey) = registration {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().pubkey(pubkey).build());
    }

    let app = builder
        .manage(AppState {
            updater_enabled: registration.is_ok(),
            ..AppState::default()
        })
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::retry,
            commands::current_instance,
            commands::open_external,
            commands::app_version,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let loaded = Config::load(&handle);
            // Only a login start comes up hidden. `start_hidden` in the store
            // records that the autostart registration exists; it must not make
            // a launch the person asked for open into an empty tray.
            let start_hidden = std::env::args().any(|arg| arg == MINIMIZED_ARG);
            {
                let state = handle.state::<AppState>();
                *state.config.lock().expect("configuration lock") = loaded;
            }

            tray::build(&handle)?;
            window::open_main_window(&handle, start_hidden)?;
            if let Err(reason) = registration {
                log::info!("update checks are disabled: {reason}");
            }
            updater::spawn_schedule(handle.clone());

            log::info!("Nexus Desktop v{} started", app.package_info().version);
            Ok(())
        })
        .build(context);

    // A failure here used to be a panic with no console to print it to and no
    // log line, since the app never reached its first `log::info!`.
    let app = match app {
        Ok(app) => app,
        Err(error) => crash::startup_failure(&error),
    };

    app.run(|_app, event| {
        // Closing the window hides it; the process keeps running so the tray
        // stays available and the update schedule keeps its timer. An explicit
        // exit carries a code and is honoured.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
