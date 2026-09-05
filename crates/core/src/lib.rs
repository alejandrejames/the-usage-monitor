//! Platform-agnostic core for ClaudeUsage.
//!
//! Everything here is pure logic: no UI, no HTTP client, no OS calls. The host
//! (Tauri, or the Phase 2 CLI probe) performs I/O and hands the results in, so
//! this crate stays unit-testable on every platform.
//!
//! Ported from the Swift sources under `Shared/`. See `docs/cross-platform.md`
//! for the port plan and the constraints that shaped it.
//!
//! `credentials` is the one exception to "no OS calls": credential storage
//! genuinely differs per platform, so it carries the only `cfg` branches.

pub mod alerts;
pub mod credentials;
pub mod model;
pub mod status;
pub mod usage;

pub use alerts::{Alert, AlertSettings};
pub use credentials::{CachedCredentials, CredentialError, Credentials, SourceKind};
pub use model::{
    all_operational, overall_health, ServiceHealth, ServiceStatus, UsageLevel, UsageSnapshot,
};

/// Default seconds between usage polls. User-configurable in the app; this is
/// the fallback when nothing is stored, matching `UsagePoller.interval`.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 60;

// MARK: - Retry backoff

/// Backoff schedule for failed polls.
///
/// This replaces the Swift `NWPathMonitor` reachability watcher, which
/// re-polled the moment connectivity returned. There is no clean
/// cross-platform equivalent, and backoff recovers nearly as fast for a
/// fraction of the complexity (see docs/cross-platform.md).
///
/// Doubles from a 10-second floor up to the normal poll interval — once it
/// reaches the steady-state cadence there is nothing to gain by waiting longer.
pub fn backoff_secs(consecutive_failures: u32) -> u64 {
    const FLOOR: u64 = 10;
    if consecutive_failures == 0 {
        return FLOOR;
    }
    // Saturating shift keeps a long outage from overflowing into a tiny delay.
    let doubled = FLOOR.saturating_mul(1u64.checked_shl(consecutive_failures).unwrap_or(u64::MAX));
    doubled.min(DEFAULT_POLL_INTERVAL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_starts_at_the_floor_and_doubles() {
        assert_eq!(backoff_secs(0), 10);
        assert_eq!(backoff_secs(1), 20);
        assert_eq!(backoff_secs(2), 40);
    }

    #[test]
    fn backoff_is_capped_at_the_poll_interval() {
        assert_eq!(backoff_secs(3), 60);
        assert_eq!(backoff_secs(10), 60);
    }

    #[test]
    fn backoff_survives_an_absurd_failure_count() {
        // A machine asleep for days must not wrap around to a 0-second delay.
        assert_eq!(backoff_secs(u32::MAX), DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(backoff_secs(64), DEFAULT_POLL_INTERVAL_SECS);
    }
}
