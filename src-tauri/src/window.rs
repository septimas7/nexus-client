//! The one window: what it loads, where it may go, and what closing it means.

use std::sync::atomic::Ordering;
use std::time::Duration;

use nexus_desktop_core::{Navigation, policy};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::config::WindowState;
use crate::instance::InstanceStatus;
use crate::{AppState, instance};

/// The single window label; every other module looks the window up by it.
pub const MAIN_WINDOW: &str = "main";

pub const CONNECT_PAGE: &str = "connect.html";
pub const UNREACHABLE_PAGE: &str = "unreachable.html";
pub const UPDATING_PAGE: &str = "updating.html";

/// Default and minimum window size (Shell UI Design Requirements §2).
const DEFAULT_SIZE: (f64, f64) = (1280.0, 840.0);
const MINIMUM_SIZE: (f64, f64) = (720.0, 480.0);

/// How long a page load on the instance origin may take before the shell goes
/// and asks `/healthz` whether the instance is still there. A cold portal load
/// over a tailnet is well inside this.
const PAGE_LOAD_GRACE: Duration = Duration::from_secs(12);

/// The URL of one of the shell's own bundled pages.
///
/// This has to match the protocol Tauri registers for bundled assets. On
/// Windows the web view cannot take a custom scheme for a top level document,
/// so Tauri serves the bundle from `http://tauri.localhost`; everywhere else it
/// is the `tauri://` custom protocol. The window is built with
/// `use_https_scheme(false)` so the Windows spelling is not ambiguous.
pub fn local_page(page: &str) -> Url {
    #[cfg(windows)]
    const BASE: &str = "http://tauri.localhost/";
    #[cfg(not(windows))]
    const BASE: &str = "tauri://localhost/";

    Url::parse(BASE)
        .and_then(|base| base.join(page))
        .expect("the bundled page base URL is a constant and always parses")
}

/// Create the main window.
///
/// When an instance is bound the window opens straight on the portal so the
/// first paint is the thing the person came for (NFR-001); the probe runs
/// alongside it and demotes the window to the unreachable page if the instance
/// is not actually there.
pub fn open_main_window<R: Runtime>(
    app: &AppHandle<R>,
    start_hidden: bool,
) -> tauri::Result<WebviewWindow<R>> {
    let (bound, saved_window) = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        (config.instance_url.clone(), config.window)
    };

    let target = match &bound {
        Some(url) => WebviewUrl::External(url.clone()),
        None => WebviewUrl::App(CONNECT_PAGE.into()),
    };

    let navigation_app = app.clone();
    let page_load_app = app.clone();

    let mut builder = WebviewWindowBuilder::new(app, MAIN_WINDOW, target)
        .title("Nexus Desktop")
        .inner_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1)
        .min_inner_size(MINIMUM_SIZE.0, MINIMUM_SIZE.1)
        .visible(!start_hidden)
        .use_https_scheme(false)
        .on_navigation(move |url| decide(&navigation_app, url))
        .on_page_load(move |_window, payload| {
            on_page_load(&page_load_app, payload.event(), payload.url().clone())
        });

    if let Some(state) = saved_window {
        builder = builder
            .position(f64::from(state.x), f64::from(state.y))
            .inner_size(f64::from(state.width), f64::from(state.height));
    }

    let window = builder.build()?;

    let event_app = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide_to_tray(&event_app);
        }
    });

    if let Some(url) = bound {
        let probe_app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = instance::probe(&url).await {
                log::warn!("the bound instance did not answer at launch: {error}");
                mark_unreachable(&probe_app, error.to_string());
            }
        });
    }

    Ok(window)
}

/// Apply the navigation policy. Returning `false` cancels the navigation.
fn decide<R: Runtime>(app: &AppHandle<R>, url: &Url) -> bool {
    let bound = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        config.instance_url.clone()
    };

    match policy(bound.as_ref(), url) {
        Navigation::Allow => true,
        Navigation::OpenExternally => {
            log::info!("opening {} in the system browser", url.as_str());
            if let Err(error) = app.opener().open_url(url.as_str(), None::<&str>) {
                log::error!("could not open {}: {error}", url.as_str());
            }
            false
        }
        Navigation::Block => {
            log::warn!("blocked a navigation to {}", url.scheme());
            false
        }
    }
}

/// A page load on the instance origin that never finishes means the instance
/// went away mid session. The web view has no load-failure callback, so the
/// shell arms a timer on every start and disarms it on the matching finish.
fn on_page_load<R: Runtime>(app: &AppHandle<R>, event: tauri::webview::PageLoadEvent, url: Url) {
    let state = app.state::<AppState>();
    let bound = {
        let config = state.config.lock().expect("configuration lock");
        config.instance_url.clone()
    };
    let Some(bound) = bound else { return };
    if !nexus_desktop_core::navigation::same_origin(&bound, &url) {
        return;
    }

    match event {
        tauri::webview::PageLoadEvent::Started => {
            let generation = state.load_generation.fetch_add(1, Ordering::SeqCst) + 1;
            *state.pending_load.lock().expect("pending load lock") = Some(generation);

            let watchdog_app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(PAGE_LOAD_GRACE).await;
                let still_pending = {
                    let state = watchdog_app.state::<AppState>();
                    let pending = state.pending_load.lock().expect("pending load lock");
                    *pending == Some(generation)
                };
                if !still_pending {
                    return;
                }
                log::warn!("the portal did not finish loading; asking the instance for health");
                if let Err(error) = instance::probe(&bound).await {
                    mark_unreachable(&watchdog_app, error.to_string());
                }
            });
        }
        tauri::webview::PageLoadEvent::Finished => {
            *state.pending_load.lock().expect("pending load lock") = None;
            let mut status = state.instance.lock().expect("instance lock");
            if matches!(*status, InstanceStatus::Unreachable { .. }) {
                *status = InstanceStatus::Unbound;
            }
        }
    }
}

/// Record the failure and put the unreachable page in the window.
pub fn mark_unreachable<R: Runtime>(app: &AppHandle<R>, reason: String) {
    {
        let state = app.state::<AppState>();
        *state.pending_load.lock().expect("pending load lock") = None;
        *state.instance.lock().expect("instance lock") = InstanceStatus::Unreachable { reason };
    }
    show_local_page(app, UNREACHABLE_PAGE);
    crate::tray::refresh(app);
}

/// Point the window at the instance's portal.
pub fn show_portal<R: Runtime>(app: &AppHandle<R>, url: &Url) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if let Err(error) = window.navigate(url.clone()) {
            log::error!("could not load the portal: {error}");
        }
    } else if let Err(error) = open_main_window(app, false) {
        log::error!("could not create the window: {error}");
    }
}

/// Point the window at one of the shell's own pages.
pub fn show_local_page<R: Runtime>(app: &AppHandle<R>, page: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if let Err(error) = window.navigate(local_page(page)) {
            log::error!("could not load {page}: {error}");
        }
    } else if let Err(error) = open_main_window(app, false) {
        log::error!("could not create the window: {error}");
    }
}

/// Bring the window up, creating it if a previous quit or a hidden start left
/// none. This is the tray's "Open Nexus", a left click on the tray icon, and a
/// second launch of the app.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    let window = match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => window,
        None => match open_main_window(app, false) {
            Ok(window) => window,
            Err(error) => {
                log::error!("could not create the window: {error}");
                return;
            }
        },
    };

    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// FR-007: closing the window leaves Nexus running in the tray. The first time
/// it happens the shell says so once, and never again.
fn hide_to_tray<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        remember_window(app, &window);
        let _ = window.hide();
    }

    let already_shown = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        config.hint_shown
    };
    if already_shown {
        return;
    }

    {
        let state = app.state::<AppState>();
        let mut config = state.config.lock().expect("configuration lock");
        config.hint_shown = true;
        if let Err(reason) = config.save(app) {
            log::error!("could not record the tray hint: {reason}");
        }
    }

    app.dialog()
        .message("Nexus keeps running in the tray. Quit from the tray icon.")
        .title("Nexus Desktop")
        .show(|_| {});
}

/// Save the window geometry in logical units, so a move between monitors of
/// different scaling does not shrink or grow the window on the next run.
pub fn remember_window<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };

    let position: LogicalPosition<f64> = position.to_logical(scale);
    let size: LogicalSize<f64> = size.to_logical(scale);
    if size.width < 1.0 || size.height < 1.0 {
        return;
    }

    let state = app.state::<AppState>();
    let mut config = state.config.lock().expect("configuration lock");
    config.window = Some(WindowState {
        x: position.x.round() as i32,
        y: position.y.round() as i32,
        width: size.width.round() as u32,
        height: size.height.round() as u32,
    });
    if let Err(reason) = config.save(app) {
        log::error!("could not save the window position: {reason}");
    }
}
