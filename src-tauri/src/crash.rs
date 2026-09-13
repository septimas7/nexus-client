//! Reporting a shell that cannot start.
//!
//! A release build has no console, so an error from `tauri::Builder::build` or
//! a panic used to end the process with nothing on the screen and nothing in
//! the log: the log plugin writes what the app logs, and a panic is not logged.
//! v0.1.6 died that way on every launch (see `updater::registration`). Every
//! such failure now goes to three places: standard error, `crash.log` next to
//! the ordinary log file, and a native message box that names that file.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread::ThreadId;

/// The file the failures go to, in the app's log directory.
pub const CRASH_LOG: &str = "crash.log";

static LOG_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Install the panic hook. Called before the app is built, so the hook covers
/// the plugins' own initialisation as well as everything after it.
pub fn install(config: &tauri::Config) {
    let _ = LOG_DIR.set(log_dir(&config.identifier));
    let _ = MAIN_THREAD.set(std::thread::current().id());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report(&describe_panic(info));
        default_hook(info);
    }));
}

/// Record a startup failure, show it, and end the process.
pub fn startup_failure(error: &dyn std::fmt::Display) -> ! {
    report(&format!("Nexus Desktop could not start: {error}"));
    std::process::exit(1)
}

fn report(message: &str) {
    let _ = writeln!(std::io::stderr(), "{message}");
    let recorded = record(message);
    // A dialog needs the UI thread; a panic on a worker thread is recorded and
    // left to the default hook.
    if on_main_thread() {
        show(message, recorded.as_deref());
    }
}

/// Append the message to `crash.log`; the path when that worked.
fn record(message: &str) -> Option<PathBuf> {
    let dir = LOG_DIR.get()?.as_deref()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(CRASH_LOG);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    writeln!(file, "{stamp} v{} {message}", env!("CARGO_PKG_VERSION")).ok()?;
    Some(path)
}

fn on_main_thread() -> bool {
    MAIN_THREAD
        .get()
        .is_some_and(|id| *id == std::thread::current().id())
}

fn show(message: &str, recorded: Option<&Path>) {
    let mut text = message.to_string();
    match recorded {
        Some(path) => {
            let _ = write!(text, "\n\nThis was written to {}.", path.display());
        }
        None => text.push_str("\n\nThis could not be written to the log folder."),
    }
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Nexus Desktop")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn describe_panic(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let what = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("panic");
    match info.location() {
        Some(location) => format!(
            "Nexus Desktop stopped unexpectedly: {what} (at {}:{})",
            location.file(),
            location.line()
        ),
        None => format!("Nexus Desktop stopped unexpectedly: {what}"),
    }
}

/// Where Tauri's `app_log_dir` points, worked out without an app handle:
/// `%LOCALAPPDATA%\<identifier>\logs` on Windows, `~/Library/Logs/<identifier>`
/// on macOS, and `$XDG_DATA_HOME/<identifier>/logs` elsewhere.
fn log_dir(identifier: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|base| PathBuf::from(base).join(identifier).join("logs"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join(identifier)
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
            .map(|base| base.join(identifier).join("logs"))
    }
}
