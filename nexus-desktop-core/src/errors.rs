//! The errors the shell shows a person, carrying the exact copy of the Shell UI
//! Design Requirements (§7 Content and copy). `Display` is the sentence the UI
//! renders; there is no second place where these strings are written.

use thiserror::Error;

/// Everything that can go wrong between a typed address and a working window.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DesktopError {
    /// The text is not an address at all, or asks for a scheme the shell
    /// refuses (anything but https, or plain http outside a private network).
    #[error("That does not look like an address.")]
    InvalidUrl,

    /// The instance did not answer, or refused. `reason` is a whole sentence
    /// built by one of the constructors below so the host name appears in it.
    #[error("{reason}")]
    InstanceUnreachable { reason: String },

    /// Something answered on that address, but it is not a Nexus instance.
    #[error("{host} answered, but it is not a Nexus instance.")]
    InstanceNotNexus { host: String },

    /// An update was accepted and then failed to download or install.
    #[error("The update could not be installed: {reason}. Try again later.")]
    UpdateFailed { reason: String },
}

impl DesktopError {
    /// The probe timed out or the name did not resolve.
    pub fn did_not_answer(host: &str) -> Self {
        Self::InstanceUnreachable {
            reason: format!("{host} did not answer."),
        }
    }

    /// The host is there and actively refused the connection.
    pub fn refused(host: &str) -> Self {
        Self::InstanceUnreachable {
            reason: format!("{host} refused the connection."),
        }
    }

    /// Something answered but the body is not a Nexus health document.
    pub fn not_nexus(host: &str) -> Self {
        Self::InstanceNotNexus {
            host: host.to_string(),
        }
    }

    /// An update failed; `reason` is whatever the updater reported.
    pub fn update_failed(reason: impl Into<String>) -> Self {
        Self::UpdateFailed {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_the_ui_copy() {
        assert_eq!(
            DesktopError::InvalidUrl.to_string(),
            "That does not look like an address."
        );
        assert_eq!(
            DesktopError::did_not_answer("nexus.tail-net.ts.net").to_string(),
            "nexus.tail-net.ts.net did not answer."
        );
        assert_eq!(
            DesktopError::refused("192.168.0.183:8080").to_string(),
            "192.168.0.183:8080 refused the connection."
        );
        assert_eq!(
            DesktopError::not_nexus("example.com").to_string(),
            "example.com answered, but it is not a Nexus instance."
        );
        assert_eq!(
            DesktopError::update_failed("the signature did not verify").to_string(),
            "The update could not be installed: the signature did not verify. Try again later."
        );
    }
}
