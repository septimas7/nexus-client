//! The tray icon and its menu: the way back to a hidden window, and the only
//! way to quit.
//!
//! The menu is rebuilt rather than mutated whenever something it shows changes
//! (the bound host, the autostart registration, an update becoming available),
//! which keeps the labels and the state in one function instead of two.

use nexus_desktop_core::host_label;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

use crate::{AppState, updater, window};

/// The tray icon id, so later refreshes can find it again.
pub const TRAY_ID: &str = "main";

const ID_OPEN: &str = "open";
const ID_UPDATE: &str = "update";
const ID_AUTOSTART: &str = "autostart";
const ID_CHANGE: &str = "change";
const ID_ABOUT: &str = "about";
const ID_QUIT: &str = "quit";

/// Create the tray icon. Called once, during setup.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip(app))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// Redraw the menu and the tooltip after anything they show has changed.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                log::error!("could not refresh the tray menu: {error}");
            }
        }
        Err(error) => log::error!("could not build the tray menu: {error}"),
    }
    if let Err(error) = tray.set_tooltip(Some(tooltip(app))) {
        log::error!("could not refresh the tray tooltip: {error}");
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let open = MenuItemBuilder::with_id(ID_OPEN, "Open Nexus").build(app)?;

    let pending = {
        let state = app.state::<AppState>();
        let pending = state.pending_update.lock().expect("pending update lock");
        pending.clone()
    };
    let update = if updater::is_enabled() {
        let label = match &pending {
            Some(version) => format!("Update to v{version}\u{2026}"),
            None => "Check for updates\u{2026}".to_string(),
        };
        MenuItemBuilder::with_id(ID_UPDATE, label).build(app)?
    } else {
        MenuItemBuilder::with_id(ID_UPDATE, "Updates unavailable in this build")
            .enabled(false)
            .build(app)?
    };

    let autostart = CheckMenuItemBuilder::with_id(ID_AUTOSTART, "Start at login")
        .checked(app.autolaunch().is_enabled().unwrap_or(false))
        .build(app)?;

    let change = MenuItemBuilder::with_id(ID_CHANGE, "Change instance\u{2026}").build(app)?;
    let about = MenuItemBuilder::with_id(ID_ABOUT, "About Nexus Desktop").build(app)?;
    let quit = MenuItemBuilder::with_id(ID_QUIT, "Quit").build(app)?;

    MenuBuilder::new(app)
        .item(&open)
        .item(&update)
        .item(&autostart)
        .item(&change)
        .separator()
        .item(&about)
        .item(&quit)
        .build()
}

/// "Nexus Desktop v0.1.0 - nexus.tail-net.ts.net", or "Not connected".
fn tooltip<R: Runtime>(app: &AppHandle<R>) -> String {
    let version = app.package_info().version.to_string();
    let state = app.state::<AppState>();
    let config = state.config.lock().expect("configuration lock");
    match &config.instance_url {
        Some(url) => format!("Nexus Desktop v{version} \u{00b7} {}", host_label(url)),
        None => format!("Nexus Desktop v{version} \u{00b7} Not connected"),
    }
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        ID_OPEN => window::show(app),
        ID_UPDATE => {
            let update_app = app.clone();
            tauri::async_runtime::spawn(async move {
                updater::check_and_prompt(update_app, true).await;
            });
        }
        ID_AUTOSTART => toggle_autostart(app),
        ID_CHANGE => {
            window::show(app);
            window::show_local_page(app, window::CONNECT_PAGE);
        }
        ID_ABOUT => show_about(app),
        ID_QUIT => {
            if let Some(window) = app.get_webview_window(window::MAIN_WINDOW) {
                window::remember_window(app, &window);
            }
            app.exit(0);
        }
        other => log::warn!("unhandled tray menu item {other}"),
    }
}

/// Register or unregister the app with the OS login items. The `--minimized`
/// argument is attached when the autostart plugin is initialised, so a login
/// launch comes up in the tray rather than as a window.
fn toggle_autostart<R: Runtime>(app: &AppHandle<R>) {
    let manager = app.autolaunch();
    let enabled = manager.is_enabled().unwrap_or(false);

    let result = if enabled {
        manager.disable()
    } else {
        manager.enable()
    };
    match result {
        Ok(()) => {
            let state = app.state::<AppState>();
            let mut config = state.config.lock().expect("configuration lock");
            config.start_hidden = !enabled;
            if let Err(reason) = config.save(app) {
                log::error!("could not save the start at login setting: {reason}");
            }
        }
        Err(error) => log::error!("could not change the start at login setting: {error}"),
    }

    refresh(app);
}

fn show_about<R: Runtime>(app: &AppHandle<R>) {
    let version = app.package_info().version.to_string();
    let connected = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        match &config.instance_url {
            Some(url) => format!("Connected to {}", host_label(url)),
            None => "Not connected".to_string(),
        }
    };
    let logs = app
        .path()
        .app_log_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unavailable".to_string());

    app.dialog()
        .message(format!(
            "Nexus Desktop v{version}\n{connected}\nLogs: {logs}"
        ))
        .title("About Nexus Desktop")
        .show(|_| {});
}
