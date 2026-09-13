//! When to look for a new release.
//!
//! The shell checks once shortly after launch and then on a slow timer. Both
//! numbers live here so the schedule is testable without a running app and
//! without waiting six hours.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

/// How long after launch the first check runs. Long enough that the window has
/// painted and the portal has loaded, short enough that a person who opened the
/// app to get the update gets it.
pub const LAUNCH_DELAY: Duration = Duration::from_secs(15);

/// The gap between checks while the app keeps running. Closing the window hides
/// it to the tray, so this timer survives an ordinary day of use.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// Whether an update check is due.
///
/// `last_check` is the timestamp the configuration store holds. A missing one
/// means this install has never checked, so it is due. A timestamp in the
/// future means the stored value or the clock is wrong, and waiting six hours
/// past a wrong future time would silently disable updates, so that is due as
/// well.
pub fn schedule(now: DateTime<Utc>, last_check: Option<DateTime<Utc>>) -> bool {
    let Some(last) = last_check else {
        return true;
    };
    let elapsed = now.signed_duration_since(last);
    elapsed < TimeDelta::zero() || elapsed >= interval()
}

fn interval() -> TimeDelta {
    TimeDelta::from_std(CHECK_INTERVAL).expect("the six hour interval fits in a TimeDelta")
}

/// The updater public key a build was given, or `None` when it was given
/// nothing usable.
///
/// The shell compiles the key in from the `NEXUS_DESKTOP_UPDATER_PUBKEY` build
/// variable. A CI job that exports that variable from an unset repository
/// variable exports it as an empty string, and `option_env!` reports an empty
/// but present variable as `Some("")`. v0.1.6 shipped that way: the empty key
/// registered the updater plugin against a configuration with no updater
/// section, and the shell died during startup on every launch. Blank is
/// therefore the same as absent, and surrounding whitespace (a pasted key with
/// a trailing newline) is dropped so the key reaches the verifier as written.
pub const fn configured_pubkey(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        Some(key) => {
            let key = key.trim_ascii();
            if key.is_empty() { None } else { Some(key) }
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .expect("test timestamp should parse")
            .with_timezone(&Utc)
    }

    #[test]
    fn an_install_that_has_never_checked_is_due() {
        assert!(schedule(at("2026-09-13T10:00:00Z"), None));
    }

    #[test]
    fn a_check_inside_the_window_is_not_due() {
        let now = at("2026-09-13T10:00:00Z");
        assert!(!schedule(now, Some(at("2026-09-13T09:59:59Z"))));
        assert!(!schedule(now, Some(at("2026-09-13T04:00:01Z"))));
        assert!(!schedule(now, Some(now)));
    }

    #[test]
    fn six_hours_later_is_due() {
        let now = at("2026-09-13T10:00:00Z");
        assert!(schedule(now, Some(at("2026-09-13T04:00:00Z"))));
        assert!(schedule(now, Some(at("2026-09-12T22:00:00Z"))));
        assert!(schedule(now, Some(at("2025-01-01T00:00:00Z"))));
    }

    #[test]
    fn a_timestamp_from_the_future_does_not_disable_updates() {
        let now = at("2026-09-13T10:00:00Z");
        assert!(schedule(now, Some(at("2027-01-01T00:00:00Z"))));
        assert!(schedule(now, Some(at("2026-09-13T10:00:01Z"))));
    }

    #[test]
    fn the_intervals_are_the_ones_the_design_states() {
        assert_eq!(LAUNCH_DELAY.as_secs(), 15);
        assert_eq!(CHECK_INTERVAL.as_secs(), 21_600);
    }

    #[test]
    fn an_absent_or_blank_key_disables_the_updater() {
        assert_eq!(configured_pubkey(None), None);
        assert_eq!(configured_pubkey(Some("")), None);
        assert_eq!(configured_pubkey(Some(" \n\t")), None);
    }

    #[test]
    fn a_key_is_kept_without_its_surrounding_whitespace() {
        assert_eq!(
            configured_pubkey(Some("dW50cnVzdGVkIGNvbW1lbnQ=\n")),
            Some("dW50cnVzdGVkIGNvbW1lbnQ=")
        );
        assert_eq!(configured_pubkey(Some("abc")), Some("abc"));
    }

    const COMPILED: Option<&str> = configured_pubkey(Some(" k "));

    #[test]
    fn the_filter_is_usable_in_a_constant() {
        assert_eq!(COMPILED, Some("k"));
    }
}
