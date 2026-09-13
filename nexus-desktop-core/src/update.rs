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
}
