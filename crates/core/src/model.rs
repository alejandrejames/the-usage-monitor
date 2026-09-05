//! Shared value types: usage snapshots, threshold colours, service health.

use serde::{Deserialize, Serialize};

// MARK: - Usage level

/// Semantic colour bucket for a usage percentage. Ported from `Theme.swift`'s
/// `Double.usageColor`: green below 70, amber below 90, red at or above 90.
///
/// The bucket — not the raw percentage — is what drives the Linux tray icon
/// re-render, because `set_icon` there writes a PNG to disk on every call
/// (see docs/cross-platform.md). Comparing buckets keeps that to a handful of
/// writes a day instead of one per poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageLevel {
    Green,
    Amber,
    Red,
}

impl UsageLevel {
    /// Bucket for a percentage in 0–100. Boundaries are inclusive-upward:
    /// exactly 70 is amber and exactly 90 is red, matching `Theme.swift`.
    pub fn for_percent(percent: f64) -> Self {
        if percent < 70.0 {
            Self::Green
        } else if percent < 90.0 {
            Self::Amber
        } else {
            Self::Red
        }
    }

    /// Hex colour, matching the asset catalog entries the Swift app uses.
    pub fn hex(self) -> &'static str {
        match self {
            Self::Green => "#1D9E75",
            Self::Amber => "#BA7517",
            Self::Red => "#E24B4A",
        }
    }

    /// Straight RGB, for the tray renderer which composites raw pixels.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Green => [0x1D, 0x9E, 0x75],
            Self::Amber => [0xBA, 0x75, 0x17],
            Self::Red => [0xE2, 0x4B, 0x4A],
        }
    }
}

// MARK: - Usage snapshot

/// One reading of both rate-limit windows.
///
/// Reset times are epoch **milliseconds** rather than a date type on purpose:
/// formatting happens in the WebView via `Intl.DateTimeFormat`, which has full
/// locale data for free. Rust has no locale-aware date formatting without
/// pulling in `icu`. See docs/cross-platform.md.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    /// 5-hour "session" window, 0–100.
    pub session_percent: f64,
    /// 7-day "weekly" window, 0–100.
    pub weekly_percent: f64,
    /// When the session window resets, epoch millis.
    pub session_reset_at_ms: Option<i64>,
    /// When the weekly window resets, epoch millis.
    pub weekly_reset_at_ms: Option<i64>,
}

impl UsageSnapshot {
    pub fn session_level(&self) -> UsageLevel {
        UsageLevel::for_percent(self.session_percent)
    }

    pub fn weekly_level(&self) -> UsageLevel {
        UsageLevel::for_percent(self.weekly_percent)
    }
}

// MARK: - Service health

/// Health of one Claude service, mapped from Statuspage's component status.
/// Ported from `StatusStore.swift`'s `ServiceHealth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceHealth {
    Operational,
    Degraded,
    PartialOutage,
    MajorOutage,
    Maintenance,
    Unknown,
}

impl ServiceHealth {
    /// Statuspage component `status` strings to our cases. Anything
    /// unrecognised is `Unknown` rather than an error — an upstream addition
    /// must not blank the row.
    pub fn from_api(value: &str) -> Self {
        match value {
            "operational" => Self::Operational,
            "degraded_performance" => Self::Degraded,
            "partial_outage" => Self::PartialOutage,
            "major_outage" => Self::MajorOutage,
            "under_maintenance" => Self::Maintenance,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Operational => "Operational",
            Self::Degraded => "Degraded",
            Self::PartialOutage => "Partial outage",
            Self::MajorOutage => "Major outage",
            Self::Maintenance => "Maintenance",
            Self::Unknown => "Unknown",
        }
    }

    /// True for anything the user would want to notice at a glance.
    pub fn is_healthy(self) -> bool {
        self == Self::Operational
    }

    /// Severity for "worst of" comparison. Higher is worse. `Unknown` sits
    /// below every real problem so a missing component cannot outrank a
    /// genuine outage.
    fn severity(self) -> u8 {
        match self {
            Self::Operational => 0,
            Self::Unknown => 1,
            Self::Maintenance => 2,
            Self::Degraded => 3,
            Self::PartialOutage => 4,
            Self::MajorOutage => 5,
        }
    }
}

/// One row in the status accordion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Statuspage component id.
    pub id: String,
    /// Display name.
    pub name: String,
    pub health: ServiceHealth,
}

/// Worst health across a set of services, driving the collapsed summary.
///
/// `StatusStore.swift` hardcodes the precedence major > partial > degraded >
/// maintenance, with empty meaning unknown. This reproduces that ordering via
/// `severity()` so adding a case cannot silently break the precedence.
pub fn overall_health(services: &[ServiceStatus]) -> ServiceHealth {
    services.iter().map(|s| s.health).max_by_key(|h| h.severity()).unwrap_or(ServiceHealth::Unknown)
}

/// True when every tracked service is operational. False for an empty list,
/// matching `StatusStore.allOperational`.
pub fn all_operational(services: &[ServiceStatus]) -> bool {
    !services.is_empty() && overall_health(services) == ServiceHealth::Operational
}

#[cfg(test)]
mod tests {
    use super::*;

    // MARK: UsageLevel boundaries

    #[test]
    fn usage_level_boundaries_are_inclusive_upward() {
        // The exact boundary values are the whole point: Theme.swift uses
        // `..<70` and `..<90`, so 70 is amber and 90 is red, not the reverse.
        assert_eq!(UsageLevel::for_percent(0.0), UsageLevel::Green);
        assert_eq!(UsageLevel::for_percent(69.9), UsageLevel::Green);
        assert_eq!(UsageLevel::for_percent(70.0), UsageLevel::Amber);
        assert_eq!(UsageLevel::for_percent(89.9), UsageLevel::Amber);
        assert_eq!(UsageLevel::for_percent(90.0), UsageLevel::Red);
        assert_eq!(UsageLevel::for_percent(100.0), UsageLevel::Red);
    }

    #[test]
    fn usage_level_handles_out_of_range() {
        // Utilization headers should stay in 0–1, but a >100% reading must
        // still bucket as red rather than panic.
        assert_eq!(UsageLevel::for_percent(-5.0), UsageLevel::Green);
        assert_eq!(UsageLevel::for_percent(150.0), UsageLevel::Red);
    }

    #[test]
    fn usage_level_hex_matches_rgb() {
        for level in [UsageLevel::Green, UsageLevel::Amber, UsageLevel::Red] {
            let [r, g, b] = level.rgb();
            assert_eq!(format!("#{r:02X}{g:02X}{b:02X}"), level.hex());
        }
    }

    // MARK: ServiceHealth mapping

    #[test]
    fn service_health_maps_every_known_status() {
        assert_eq!(ServiceHealth::from_api("operational"), ServiceHealth::Operational);
        assert_eq!(ServiceHealth::from_api("degraded_performance"), ServiceHealth::Degraded);
        assert_eq!(ServiceHealth::from_api("partial_outage"), ServiceHealth::PartialOutage);
        assert_eq!(ServiceHealth::from_api("major_outage"), ServiceHealth::MajorOutage);
        assert_eq!(ServiceHealth::from_api("under_maintenance"), ServiceHealth::Maintenance);
    }

    #[test]
    fn service_health_unknown_for_unrecognised() {
        assert_eq!(ServiceHealth::from_api("something_new"), ServiceHealth::Unknown);
        assert_eq!(ServiceHealth::from_api(""), ServiceHealth::Unknown);
    }

    // MARK: overall / all_operational

    fn svc(health: ServiceHealth) -> ServiceStatus {
        ServiceStatus { id: "x".into(), name: "x".into(), health }
    }

    #[test]
    fn overall_picks_the_worst() {
        assert_eq!(
            overall_health(&[svc(ServiceHealth::Operational), svc(ServiceHealth::MajorOutage)]),
            ServiceHealth::MajorOutage
        );
        assert_eq!(
            overall_health(&[svc(ServiceHealth::Degraded), svc(ServiceHealth::PartialOutage)]),
            ServiceHealth::PartialOutage
        );
        assert_eq!(
            overall_health(&[svc(ServiceHealth::Maintenance), svc(ServiceHealth::Degraded)]),
            ServiceHealth::Degraded
        );
    }

    #[test]
    fn overall_of_empty_is_unknown() {
        assert_eq!(overall_health(&[]), ServiceHealth::Unknown);
    }

    #[test]
    fn a_real_outage_outranks_unknown() {
        // A component we failed to match must never mask a live outage.
        assert_eq!(
            overall_health(&[svc(ServiceHealth::Unknown), svc(ServiceHealth::MajorOutage)]),
            ServiceHealth::MajorOutage
        );
        // But unknown does outrank plain operational, so a missing component
        // is still visible rather than reported as healthy.
        assert_eq!(
            overall_health(&[svc(ServiceHealth::Unknown), svc(ServiceHealth::Operational)]),
            ServiceHealth::Unknown
        );
    }

    #[test]
    fn all_operational_requires_a_non_empty_list() {
        assert!(!all_operational(&[]));
        assert!(all_operational(&[svc(ServiceHealth::Operational)]));
        assert!(!all_operational(&[svc(ServiceHealth::Operational), svc(ServiceHealth::Degraded)]));
    }
}
