//! Asking an address whether it is a Nexus instance, and binding to it.
//!
//! The only thing the shell knows how to read from the platform is
//! `GET /healthz`. Everything else in the window is the portal, which the shell
//! neither parses nor understands.

use std::time::Duration;

use nexus_desktop_core::{DesktopError, HealthResponse, host_label, parse_health};
use tauri::{AppHandle, Manager, Runtime};
use url::Url;

use crate::AppState;
use crate::window;

/// How long the probe waits before calling an address unreachable.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// What the shell believes about the bound instance right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum InstanceStatus {
    /// No address has been bound yet: first run, or after "Change instance".
    #[default]
    Unbound,
    /// The last probe succeeded.
    Connected { health: HealthResponse },
    /// The last probe or the last page load failed.
    Unreachable { reason: String },
}

/// Ask an address for `GET /healthz`.
///
/// A 200 carrying the platform's health document is the only accepted answer.
/// Anything else is named for the person rather than dumped as a transport
/// error, because "did not answer" and "refused the connection" are different
/// problems with different fixes.
pub async fn probe(url: &Url) -> Result<HealthResponse, DesktopError> {
    let host = host_label(url);
    let endpoint = url.join("healthz").map_err(|_| DesktopError::InvalidUrl)?;

    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .user_agent(concat!("NexusDesktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| DesktopError::InstanceUnreachable {
            reason: err.to_string(),
        })?;

    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|err| classify(&err, &host))?;

    if !response.status().is_success() {
        return Err(DesktopError::not_nexus(&host));
    }

    let body = response
        .bytes()
        .await
        .map_err(|err| classify(&err, &host))?;
    let health = parse_health(&body).ok_or_else(|| DesktopError::not_nexus(&host))?;

    if !health.is_ok() {
        log::warn!("{host} reports health status {}", health.status);
    }
    Ok(health)
}

/// A refused connection is a machine that is up with nothing listening; a
/// timeout or a name that does not resolve is a machine that did not answer.
fn classify(error: &reqwest::Error, host: &str) -> DesktopError {
    if error.is_timeout() {
        return DesktopError::did_not_answer(host);
    }

    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(io) = current.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::ConnectionRefused
        {
            return DesktopError::refused(host);
        }
        source = current.source();
    }

    DesktopError::did_not_answer(host)
}

/// Bind the client to `url`: probe it, save it, and load the portal.
///
/// Nothing is written and nothing is navigated when the probe fails, so a typo
/// on the connect page leaves the previously bound instance alone.
pub async fn switch_instance<R: Runtime>(app: &AppHandle<R>, url: Url) -> Result<(), DesktopError> {
    let health = probe(&url).await?;

    {
        let state = app.state::<AppState>();
        let mut config = state.config.lock().expect("configuration lock");
        config.instance_url = Some(url.clone());
        if let Err(reason) = config.save(app) {
            log::error!("could not save the instance address: {reason}");
        }
        *state.instance.lock().expect("instance lock") = InstanceStatus::Connected { health };
    }

    log::info!("bound to {}", host_label(&url));
    window::show_portal(app, &url);
    crate::tray::refresh(app);
    Ok(())
}

/// Re-probe the bound instance and go back to the portal when it answers. This
/// is the "Retry" button and the unreachable page's own 15 second timer.
pub async fn retry_bound_instance<R: Runtime>(app: &AppHandle<R>) -> Result<(), DesktopError> {
    let bound = {
        let state = app.state::<AppState>();
        let config = state.config.lock().expect("configuration lock");
        config.instance_url.clone()
    };

    let Some(url) = bound else {
        window::show_local_page(app, window::CONNECT_PAGE);
        return Ok(());
    };

    match probe(&url).await {
        Ok(health) => {
            let state = app.state::<AppState>();
            *state.instance.lock().expect("instance lock") = InstanceStatus::Connected { health };
            window::show_portal(app, &url);
            crate::tray::refresh(app);
            Ok(())
        }
        Err(error) => {
            let state = app.state::<AppState>();
            *state.instance.lock().expect("instance lock") = InstanceStatus::Unreachable {
                reason: error.to_string(),
            };
            crate::tray::refresh(app);
            Err(error)
        }
    }
}
