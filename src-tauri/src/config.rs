//! The one file the client owns: `nexus-desktop.json` in the OS application
//! data directory, written through `tauri-plugin-store`.
//!
//! It holds where the instance is, how the window last looked, and when the
//! updater last ran. It never holds a credential: the session is the instance's
//! own `nxs_session` cookie, which lives in the web view's cookie jar at the
//! instance origin and is `HttpOnly`, so the shell cannot read it even if it
//! wanted to.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use url::Url;

/// The store file name, relative to the app data directory.
pub const STORE_FILE: &str = "nexus-desktop.json";

const KEY_INSTANCE_URL: &str = "instance_url";
const KEY_START_HIDDEN: &str = "start_hidden";
const KEY_WINDOW: &str = "window";
const KEY_LAST_UPDATE_CHECK: &str = "last_update_check";
const KEY_HINT_SHOWN: &str = "hint_shown";

/// Where the window was and how big it was, per machine.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Everything the client remembers between runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub instance_url: Option<Url>,
    pub start_hidden: bool,
    pub window: Option<WindowState>,
    pub last_update_check: Option<DateTime<Utc>>,
    pub hint_shown: bool,
}

impl Config {
    /// Read the store. A missing, unreadable, or partly corrupt file is not an
    /// error worth showing anyone: the client falls back to first-run defaults,
    /// which is the connect page.
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let Ok(store) = app.store(STORE_FILE) else {
            log::warn!("configuration store could not be opened; using defaults");
            return Self::default();
        };

        Self {
            instance_url: store
                .get(KEY_INSTANCE_URL)
                .and_then(|value| value.as_str().and_then(|raw| Url::parse(raw).ok())),
            start_hidden: store
                .get(KEY_START_HIDDEN)
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            window: store
                .get(KEY_WINDOW)
                .and_then(|value| serde_json::from_value::<WindowState>(value).ok()),
            last_update_check: store.get(KEY_LAST_UPDATE_CHECK).and_then(|value| {
                value
                    .as_str()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|stamp| stamp.with_timezone(&Utc))
            }),
            hint_shown: store
                .get(KEY_HINT_SHOWN)
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }
    }

    /// Write every key and flush. Each key is written separately so the file on
    /// disk stays the flat document the Technical Design describes.
    pub fn save<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let store = app.store(STORE_FILE).map_err(|err| err.to_string())?;

        match &self.instance_url {
            Some(url) => store.set(KEY_INSTANCE_URL, url.as_str()),
            None => {
                store.delete(KEY_INSTANCE_URL);
            }
        }
        store.set(KEY_START_HIDDEN, self.start_hidden);
        match &self.window {
            Some(window) => store.set(
                KEY_WINDOW,
                serde_json::to_value(window).map_err(|err| err.to_string())?,
            ),
            None => {
                store.delete(KEY_WINDOW);
            }
        }
        match &self.last_update_check {
            Some(stamp) => store.set(KEY_LAST_UPDATE_CHECK, stamp.to_rfc3339()),
            None => {
                store.delete(KEY_LAST_UPDATE_CHECK);
            }
        }
        store.set(KEY_HINT_SHOWN, self.hint_shown);

        store.save().map_err(|err| err.to_string())
    }
}
