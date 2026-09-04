//! Application state and the polling loops.
//!
//! Replaces `UsageStore` / `UsagePoller` / `StatusPoller` from the Swift app.
//! `@Observable` has no Rust equivalent, so state lives behind a `Mutex` and
//! the UI is notified explicitly via a Tauri event.

use claudeusage_core::{
    alerts, credentials::CachedCredentials, status, usage, Alert, AlertSettings, ServiceStatus,
    UsageSnapshot, DEFAULT_POLL_INTERVAL_SECS,
};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Everything the popover renders, in one payload.
///
/// Reset times stay as epoch millis; the WebView formats them with
/// `Intl.DateTimeFormat`, which has locale data Rust would need `icu` for.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub session_percent: f64,
    pub weekly_percent: f64,
    pub session_reset_at_ms: Option<i64>,
    pub weekly_reset_at_ms: Option<i64>,
    pub last_updated_ms: Option<i64>,
    /// True when the last poll failed; the tray dims and shows placeholders.
    pub is_stale: bool,
    /// False when no Claude Code credential is available at all.
    pub is_authenticated: bool,
    pub plan: Option<String>,
    pub services: Vec<ServiceStatus>,
    pub status_is_stale: bool,
}

/// Shared application state.
pub struct AppState {
    pub snapshot: Mutex<AppSnapshot>,
    pub credentials: Mutex<CachedCredentials>,
    /// Previous session percentage, for edge-triggered alerts.
    last_session_percent: Mutex<f64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            snapshot: Mutex::new(AppSnapshot::default()),
            credentials: Mutex::new(CachedCredentials::with_default_sources()),
            last_session_percent: Mutex::new(0.0),
        }
    }

    pub fn snapshot(&self) -> AppSnapshot {
        self.snapshot.lock().expect("snapshot lock").clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// MARK: - Usage polling

/// Outcome of one poll, so the caller can drive backoff and alerts.
pub enum PollOutcome {
    Updated(UsageSnapshot),
    /// Credential missing — not a network failure, so no backoff.
    Unauthenticated,
    Failed,
}

/// Performs one usage poll.
///
/// Sends a minimal `/v1/messages` request and reads the rate-limit headers;
/// the completion is discarded. See `claudeusage_core::usage`.
pub fn poll_once(state: &AppState) -> PollOutcome {
    let token = {
        let mut creds = state.credentials.lock().expect("credential lock");
        match creds.token() {
            Ok(resolved) => resolved.credentials.access_token.clone(),
            Err(_) => return PollOutcome::Unauthenticated,
        }
    };

    let mut request =
        ureq::post(usage::ENDPOINT).header("Authorization", &format!("Bearer {token}"));
    for (name, value) in usage::REQUEST_HEADERS {
        request = request.header(*name, *value);
    }

    // A 429 is a normal, useful response here: the usage headers ride along
    // with it. Only transport failures and 401s are treated as problems.
    let response = match request.send(usage::PROBE_BODY) {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(401)) => {
            // The token may have been rotated out of band by Claude Code.
            state.credentials.lock().expect("credential lock").invalidate();
            return PollOutcome::Failed;
        }
        Err(ureq::Error::StatusCode(_)) => {
            // Other status codes still carry headers, but ureq consumes the
            // response on error, so treat them as a failed poll.
            return PollOutcome::Failed;
        }
        Err(_) => return PollOutcome::Failed,
    };

    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect();

    match usage::parse_headers(&headers) {
        Some(snapshot) => PollOutcome::Updated(snapshot),
        None => PollOutcome::Failed,
    }
}

/// Applies a successful poll to the shared state, returning any alert to fire.
pub fn apply_usage(state: &AppState, snapshot: UsageSnapshot) -> Option<Alert> {
    let previous = {
        let mut last = state.last_session_percent.lock().expect("alert lock");
        let previous = *last;
        *last = snapshot.session_percent;
        previous
    };

    let plan = state.credentials.lock().expect("credential lock").plan().map(str::to_string);

    {
        let mut s = state.snapshot.lock().expect("snapshot lock");
        s.session_percent = snapshot.session_percent;
        s.weekly_percent = snapshot.weekly_percent;
        s.session_reset_at_ms = snapshot.session_reset_at_ms;
        s.weekly_reset_at_ms = snapshot.weekly_reset_at_ms;
        s.last_updated_ms = Some(now_millis());
        s.is_stale = false;
        s.is_authenticated = true;
        s.plan = plan;
    }

    alerts::check(previous, snapshot.session_percent, AlertSettings::default())
}

pub fn mark_stale(state: &AppState, authenticated: bool) {
    let mut s = state.snapshot.lock().expect("snapshot lock");
    s.is_stale = true;
    s.is_authenticated = authenticated;
}

/// Seconds to wait before the next usage poll.
pub fn next_delay(consecutive_failures: u32) -> Duration {
    Duration::from_secs(if consecutive_failures == 0 {
        DEFAULT_POLL_INTERVAL_SECS
    } else {
        claudeusage_core::backoff_secs(consecutive_failures)
    })
}

// MARK: - Status polling

/// Fetches the Claude service status. Unauthenticated, and deliberately
/// independent of usage polling: an outage is worth showing even when the
/// usage reading is unavailable.
pub fn poll_status_once(state: &Arc<AppState>) -> bool {
    let body = match ureq::get(status::ENDPOINT).header("Accept", "application/json").call() {
        Ok(mut response) => match response.body_mut().read_to_string() {
            Ok(b) => b,
            Err(_) => return false,
        },
        Err(_) => return false,
    };

    match status::parse_summary(&body) {
        Some(services) => {
            let mut s = state.snapshot.lock().expect("snapshot lock");
            s.services = services;
            s.status_is_stale = false;
            true
        }
        None => {
            state.snapshot.lock().expect("snapshot lock").status_is_stale = true;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_successful_poll_clears_staleness() {
        let state = AppState::new();
        mark_stale(&state, true);
        assert!(state.snapshot().is_stale);

        apply_usage(
            &state,
            UsageSnapshot {
                session_percent: 43.0,
                weekly_percent: 71.0,
                session_reset_at_ms: Some(1),
                weekly_reset_at_ms: Some(2),
            },
        );

        let s = state.snapshot();
        assert!(!s.is_stale);
        assert!(s.is_authenticated);
        assert_eq!(s.session_percent, 43.0);
        assert!(s.last_updated_ms.is_some());
    }

    #[test]
    fn alerts_are_edge_triggered_across_polls() {
        let state = AppState::new();
        let snap = |p: f64| UsageSnapshot {
            session_percent: p,
            weekly_percent: 0.0,
            session_reset_at_ms: None,
            weekly_reset_at_ms: None,
        };

        // Below the line: silent.
        assert_eq!(apply_usage(&state, snap(50.0)), None);
        // Crossing 80 fires once.
        assert_eq!(apply_usage(&state, snap(85.0)), Some(Alert::Eighty));
        // Staying above does not re-fire.
        assert_eq!(apply_usage(&state, snap(86.0)), None);
        // Crossing 95 fires.
        assert_eq!(apply_usage(&state, snap(96.0)), Some(Alert::NinetyFive));
    }

    #[test]
    fn backoff_applies_only_after_a_failure() {
        assert_eq!(next_delay(0).as_secs(), DEFAULT_POLL_INTERVAL_SECS);
        assert_eq!(next_delay(1).as_secs(), 20);
        // Capped at the normal cadence.
        assert_eq!(next_delay(9).as_secs(), DEFAULT_POLL_INTERVAL_SECS);
    }
}
