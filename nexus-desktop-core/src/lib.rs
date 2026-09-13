//! Pure logic shared by the Nexus Desktop shell.
//!
//! Nothing in this crate depends on Tauri, on a web view, or on an operating
//! system toolkit, so every rule the shell enforces is unit-tested on any host
//! including the Linux CI runners that carry no GUI libraries. The shell crate
//! (`nexus-desktop`) is a thin wrapper that calls into here for every decision
//! it makes: which address to bind, whether an answer came from a Nexus
//! instance, where a navigation may go, and when to look for an update.

pub mod errors;
pub mod instance;
pub mod navigation;
pub mod update;

pub use errors::DesktopError;
pub use instance::{HealthResponse, host_label, normalize_url, parse_health};
pub use navigation::{Navigation, policy};
