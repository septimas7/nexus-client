//! The commands the shell's own three pages call.
//!
//! Nothing here is reachable from the instance's portal. Tauri resolves an
//! invoke against the capabilities of the origin the web view is currently
//! showing, and `capabilities/default.json` grants these commands to local
//! content only, with no `remote` entry anywhere in the file. A page served by
//! the instance therefore has no IPC surface at all (CMP-001 decision B).

use nexus_desktop_core::{Navigation, normalize_url, policy};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::AppState;
use crate::instance;

/// Bind the client to a typed address and load its portal.
#[tauri::command]
pub async fn connect(app: AppHandle, url: String) -> Result<(), String> {
    let normalized = normalize_url(&url).map_err(|error| error.to_string())?;
    instance::switch_instance(&app, normalized)
        .await
        .map_err(|error| error.to_string())
}

/// Probe the bound instance again; on success the shell reloads the portal.
#[tauri::command]
pub async fn retry(app: AppHandle) -> Result<(), String> {
    instance::retry_bound_instance(&app)
        .await
        .map_err(|error| error.to_string())
}

/// The bound address, so the connect page can prefill it and the unreachable
/// page can name the host.
#[tauri::command]
pub fn current_instance(state: State<'_, AppState>) -> Option<String> {
    let config = state.config.lock().expect("configuration lock");
    config.instance_url.as_ref().map(|url| url.to_string())
}

/// Hand a web address to the system browser.
///
/// The same navigation policy the window uses decides this, so the command
/// cannot be turned into a way to launch an arbitrary scheme.
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|_| "That does not look like an address.".to_string())?;
    if policy(None, &parsed) == Navigation::Block {
        return Err("That address cannot be opened.".to_string());
    }
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|error| error.to_string())
}

/// The running version, for the About item and the updating page.
#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
