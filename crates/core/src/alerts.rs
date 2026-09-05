//! Edge-triggered threshold alerts for session usage.

use serde::{Deserialize, Serialize};

/// A threshold crossing worth notifying about. Ported from `AlertNotifier` in
/// `UsageStore.swift`; delivery is the host's job, this crate only decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alert {
    /// Session usage crossed 80% upward.
    Eighty,
    /// Session usage crossed 95% upward.
    NinetyFive,
}

impl Alert {
    pub fn title(self) -> &'static str {
        match self {
            Self::Eighty => "Claude usage at 80%",
            Self::NinetyFive => "Claude usage at 95%",
        }
    }

    pub fn body(self) -> &'static str {
        match self {
            Self::Eighty => "You've used 80% of your session quota.",
            Self::NinetyFive => "You've used 95% of your session quota.",
        }
    }
}

/// Which alerts the user has enabled. Both default to on, matching the
/// `@AppStorage` toggles in `SettingsView.swift` which treat "never set" as true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertSettings {
    pub alert_80: bool,
    pub alert_95: bool,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self { alert_80: true, alert_95: true }
    }
}

/// Decides whether a poll should fire an alert.
///
/// Edge-triggered: a crossing fires once, not on every poll while usage stays
/// above the line. Returns at most one alert per call — 95 takes precedence, so
/// a jump from 70 straight past both lines notifies once about the worse one,
/// exactly as the Swift `else if` chain does.
///
/// A disabled 95 alert does **not** promote the 80 alert. Swift evaluates
/// `if alert95, crossed(95) { … } else if alert80, crossed(80) { … }`, so when
/// the 95 toggle is off the first branch fails and the 80 branch is tried; but
/// when 95 is *enabled and crossed*, 80 is suppressed. Both behaviours are
/// preserved here.
pub fn check(previous: f64, current: f64, settings: AlertSettings) -> Option<Alert> {
    if settings.alert_95 && crossed(95.0, previous, current) {
        Some(Alert::NinetyFive)
    } else if settings.alert_80 && crossed(80.0, previous, current) {
        Some(Alert::Eighty)
    } else {
        None
    }
}

/// True when `current` moved up through `threshold` since `previous`.
fn crossed(threshold: f64, previous: f64, current: f64) -> bool {
    previous < threshold && current >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: AlertSettings = AlertSettings { alert_80: true, alert_95: true };

    #[test]
    fn fires_once_on_an_upward_crossing() {
        assert_eq!(check(79.0, 80.0, ON), Some(Alert::Eighty));
        assert_eq!(check(94.0, 95.0, ON), Some(Alert::NinetyFive));
    }

    #[test]
    fn does_not_refire_while_above_the_line() {
        // The whole point of edge-triggering: 85 -> 86 already alerted at 80.
        assert_eq!(check(85.0, 86.0, ON), None);
        assert_eq!(check(96.0, 97.0, ON), None);
    }

    #[test]
    fn does_not_fire_below_the_line() {
        assert_eq!(check(10.0, 20.0, ON), None);
        assert_eq!(check(70.0, 79.9, ON), None);
    }

    #[test]
    fn does_not_fire_when_usage_falls() {
        // A window reset drops usage back down; that is not a crossing.
        assert_eq!(check(96.0, 5.0, ON), None);
        assert_eq!(check(85.0, 20.0, ON), None);
    }

    #[test]
    fn boundary_is_inclusive() {
        // Exactly 80 counts as crossed, matching `current >= threshold`.
        assert_eq!(check(79.99, 80.0, ON), Some(Alert::Eighty));
        // Landing just below does not.
        assert_eq!(check(79.0, 79.99, ON), None);
    }

    #[test]
    fn ninety_five_takes_precedence_over_eighty() {
        // A single poll jumping past both lines reports only the worse one.
        assert_eq!(check(50.0, 96.0, ON), Some(Alert::NinetyFive));
    }

    #[test]
    fn disabling_ninety_five_lets_eighty_fire_on_a_double_crossing() {
        // Mirrors Swift's `else if`: with 95 off, the 80 branch is evaluated.
        let only_80 = AlertSettings { alert_80: true, alert_95: false };
        assert_eq!(check(50.0, 96.0, only_80), Some(Alert::Eighty));
    }

    #[test]
    fn disabled_alerts_stay_silent() {
        let off = AlertSettings { alert_80: false, alert_95: false };
        assert_eq!(check(50.0, 96.0, off), None);
        assert_eq!(check(79.0, 81.0, off), None);

        let only_95 = AlertSettings { alert_80: false, alert_95: true };
        assert_eq!(check(79.0, 81.0, only_95), None);
        assert_eq!(check(94.0, 96.0, only_95), Some(Alert::NinetyFive));
    }

    #[test]
    fn defaults_are_both_enabled() {
        // SettingsView treats a never-set toggle as true.
        let d = AlertSettings::default();
        assert!(d.alert_80 && d.alert_95);
    }

    #[test]
    fn a_fresh_start_at_high_usage_alerts() {
        // First poll after launch: previous is 0, so opening the app already
        // over the line does notify. This matches the Swift behaviour, where
        // `sessionPercent` starts at 0.
        assert_eq!(check(0.0, 97.0, ON), Some(Alert::NinetyFive));
    }
}
