//! User preferences, persisted to disk.
//!
//! Replaces the `@AppStorage` keys the Swift app used (`refreshInterval`,
//! `alertThreshold80`, `alertThreshold95`, `menuBarDisplay`). There is no
//! cross-platform `UserDefaults`, so this is a small JSON file in the OS's
//! config directory.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// What the tray icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayDisplay {
    /// The 5-hour session window only.
    Session,
    /// The 7-day weekly window only.
    Weekly,
    /// Both, stacked.
    Both,
}

impl Default for TrayDisplay {
    fn default() -> Self {
        Self::Both
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Seconds between usage polls.
    pub refresh_interval: u64,
    pub alert_80: bool,
    pub alert_95: bool,
    pub tray_display: TrayDisplay,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_interval: claudeusage_core::DEFAULT_POLL_INTERVAL_SECS,
            // Both alerts default on, matching the Swift toggles, which treated
            // "never set" as true.
            alert_80: true,
            alert_95: true,
            tray_display: TrayDisplay::default(),
        }
    }
}

impl Settings {
    /// Polling intervals offered in the UI, mirroring the Swift picker.
    pub const INTERVAL_CHOICES: &'static [u64] = &[30, 60, 120, 300];

    /// Clamps values that would misbehave if hand-edited in the JSON file.
    fn sanitised(mut self) -> Self {
        // A 0-second interval would spin the poller; anything under 30 would
        // burn API requests for no benefit, since the headers are coarse.
        if self.refresh_interval < 30 {
            self.refresh_interval = 30;
        } else if self.refresh_interval > 3600 {
            self.refresh_interval = 3600;
        }
        self
    }

    pub fn alerts(&self) -> claudeusage_core::AlertSettings {
        claudeusage_core::AlertSettings { alert_80: self.alert_80, alert_95: self.alert_95 }
    }
}

/// `~/.config/claudeusage/settings.json` on Linux, the platform equivalent
/// elsewhere. Hand-rolled rather than adding a `dirs` dependency for one path.
fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("ClaudeUsage").join("settings.json"))
}

/// In-memory cache, so the poll loop does not read the file every tick.
static CACHE: Mutex<Option<Settings>> = Mutex::new(None);

/// The current settings, loading from disk on first use.
pub fn get() -> Settings {
    let mut cache = CACHE.lock().expect("settings lock");
    if let Some(settings) = cache.as_ref() {
        return settings.clone();
    }
    let loaded = load_from_disk().unwrap_or_default().sanitised();
    *cache = Some(loaded.clone());
    loaded
}

fn load_from_disk() -> Option<Settings> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Persists settings and updates the cache.
///
/// A write failure is reported but not fatal — the app keeps running with the
/// new values in memory, which is better than refusing the change outright.
pub fn save(settings: Settings) -> Settings {
    let settings = settings.sanitised();
    *CACHE.lock().expect("settings lock") = Some(settings.clone());

    if let Some(path) = settings_path() {
        let write = std::fs::create_dir_all(path.parent().unwrap_or(&path))
            .and_then(|_| serde_json::to_string_pretty(&settings).map_err(std::io::Error::other))
            .and_then(|json| std::fs::write(&path, json));
        if let Err(e) = write {
            eprintln!("could not save settings: {}", e.kind());
        }
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_swift_app() {
        let s = Settings::default();
        assert_eq!(s.refresh_interval, 60);
        // The Swift toggles treated "never set" as true.
        assert!(s.alert_80 && s.alert_95);
        assert_eq!(s.tray_display, TrayDisplay::Both);
    }

    #[test]
    fn absurd_intervals_are_clamped() {
        // Guards a hand-edited file: 0 would spin the poll loop.
        let spin = Settings { refresh_interval: 0, ..Default::default() };
        assert_eq!(spin.sanitised().refresh_interval, 30);

        let forever = Settings { refresh_interval: 99_999, ..Default::default() };
        assert_eq!(forever.sanitised().refresh_interval, 3600);
    }

    #[test]
    fn every_offered_interval_survives_sanitising() {
        for &choice in Settings::INTERVAL_CHOICES {
            let s = Settings { refresh_interval: choice, ..Default::default() };
            assert_eq!(s.sanitised().refresh_interval, choice, "{choice}s was altered");
        }
    }

    #[test]
    fn partial_json_falls_back_per_field() {
        // #[serde(default)] means an older or hand-trimmed file still loads.
        let s: Settings = serde_json::from_str(r#"{"alert80": false}"#).unwrap();
        assert!(!s.alert_80);
        assert!(s.alert_95, "unspecified fields keep their default");
        assert_eq!(s.refresh_interval, 60);
    }

    #[test]
    fn unknown_fields_do_not_break_loading() {
        let s: Settings = serde_json::from_str(r#"{"somethingRemoved": 1}"#).unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn alerts_are_forwarded_to_core() {
        let s = Settings { alert_80: false, alert_95: true, ..Default::default() };
        let a = s.alerts();
        assert!(!a.alert_80 && a.alert_95);
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings {
            refresh_interval: 300,
            alert_80: false,
            alert_95: true,
            tray_display: TrayDisplay::Session,
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, back);
    }
}
