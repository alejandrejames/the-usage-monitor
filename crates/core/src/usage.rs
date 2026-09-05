//! Parsing usage out of the `anthropic-ratelimit-unified-*` response headers.

use crate::model::UsageSnapshot;

// MARK: - Header names
//
// These four headers are the entire contract with the API. The app sends a
// minimal 1-token /v1/messages request purely to read them — the completion is
// discarded. If Anthropic ever ships a dedicated usage endpoint, this module is
// the only thing that has to change (see docs/cross-platform.md).

const H_5H_UTILIZATION: &str = "anthropic-ratelimit-unified-5h-utilization";
const H_7D_UTILIZATION: &str = "anthropic-ratelimit-unified-7d-utilization";
const H_5H_RESET: &str = "anthropic-ratelimit-unified-5h-reset";
const H_7D_RESET: &str = "anthropic-ratelimit-unified-7d-reset";

/// The request body sent solely to elicit the rate-limit headers.
/// `max_tokens: 1` keeps the (discarded) completion as cheap as possible.
pub const PROBE_BODY: &str = r#"{"model":"claude-haiku-4-5-20251001","max_tokens":1,"messages":[{"role":"user","content":"."}]}"#;

/// The endpoint the probe request is sent to.
pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

/// Headers the probe request must carry, minus `Authorization` which the
/// caller adds from the credential source.
pub const REQUEST_HEADERS: &[(&str, &str)] = &[
    ("anthropic-beta", "oauth-2025-04-20"),
    ("anthropic-version", "2023-06-01"),
    ("content-type", "application/json"),
];

// MARK: - Header lookup

/// Case-insensitive header lookup.
///
/// HTTP header names are case-insensitive per RFC 9110, and the Swift version
/// got this for free from `HTTPURLResponse.allHeaderFields`. A plain HashMap
/// keyed on the wire casing would not, so normalise here rather than trusting
/// whatever the HTTP client hands back.
fn find<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn number(headers: &[(String, String)], name: &str) -> Option<f64> {
    find(headers, name)?.trim().parse::<f64>().ok()
}

/// Builds a snapshot from response headers, or `None` when either utilization
/// header is absent or unparseable.
///
/// Both utilization headers are required — a reading with only one window is
/// not displayable. Reset headers are optional; the UI shows an em dash when
/// they are missing.
///
/// Usage headers ride along on **both 200 and 429** responses, so a
/// rate-limited reply is still a valid reading. The caller must not treat 429
/// as a failure.
pub fn parse_headers(headers: &[(String, String)]) -> Option<UsageSnapshot> {
    let session = number(headers, H_5H_UTILIZATION)?;
    let weekly = number(headers, H_7D_UTILIZATION)?;

    // Reject non-finite values rather than letting NaN reach the tray renderer,
    // where it would format as "NaN%".
    if !session.is_finite() || !weekly.is_finite() {
        return None;
    }

    // Headers are fractions in 0–1; the UI wants whole percentages. Swift's
    // `.rounded()` is half-away-from-zero, which `f64::round` matches exactly.
    Some(UsageSnapshot {
        session_percent: (session * 100.0).round(),
        weekly_percent: (weekly * 100.0).round(),
        session_reset_at_ms: epoch_seconds_to_millis(number(headers, H_5H_RESET)),
        weekly_reset_at_ms: epoch_seconds_to_millis(number(headers, H_7D_RESET)),
    })
}

/// Reset headers are epoch **seconds**; the rest of the app carries epoch
/// **millis** so the WebView can pass them straight to `new Date()`.
fn epoch_seconds_to_millis(seconds: Option<f64>) -> Option<i64> {
    let s = seconds?;
    if !s.is_finite() {
        return None;
    }
    Some((s * 1000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn full() -> Vec<(String, String)> {
        h(&[
            (H_5H_UTILIZATION, "0.43"),
            (H_7D_UTILIZATION, "0.71"),
            (H_5H_RESET, "1756900000"),
            (H_7D_RESET, "1757200000"),
        ])
    }

    #[test]
    fn parses_a_complete_header_set() {
        let s = parse_headers(&full()).expect("should parse");
        assert_eq!(s.session_percent, 43.0);
        assert_eq!(s.weekly_percent, 71.0);
        assert_eq!(s.session_reset_at_ms, Some(1_756_900_000_000));
        assert_eq!(s.weekly_reset_at_ms, Some(1_757_200_000_000));
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        // Some HTTP clients normalise to Title-Case; the reading must survive.
        let headers = h(&[
            ("Anthropic-RateLimit-Unified-5h-Utilization", "0.5"),
            ("ANTHROPIC-RATELIMIT-UNIFIED-7D-UTILIZATION", "0.25"),
        ]);
        let s = parse_headers(&headers).expect("should parse");
        assert_eq!(s.session_percent, 50.0);
        assert_eq!(s.weekly_percent, 25.0);
    }

    #[test]
    fn missing_either_utilization_header_yields_none() {
        let only_session = h(&[(H_5H_UTILIZATION, "0.4")]);
        assert!(parse_headers(&only_session).is_none());

        let only_weekly = h(&[(H_7D_UTILIZATION, "0.4")]);
        assert!(parse_headers(&only_weekly).is_none());

        assert!(parse_headers(&[]).is_none());
    }

    #[test]
    fn reset_headers_are_optional() {
        // A reading without reset times is still displayable.
        let headers = h(&[(H_5H_UTILIZATION, "0.1"), (H_7D_UTILIZATION, "0.2")]);
        let s = parse_headers(&headers).expect("should parse");
        assert_eq!(s.session_reset_at_ms, None);
        assert_eq!(s.weekly_reset_at_ms, None);
    }

    #[test]
    fn unparseable_values_are_rejected() {
        let garbage = h(&[(H_5H_UTILIZATION, "abc"), (H_7D_UTILIZATION, "0.2")]);
        assert!(parse_headers(&garbage).is_none());
    }

    #[test]
    fn non_finite_values_are_rejected() {
        // "NaN" and "inf" parse successfully as f64 — they must not reach the
        // renderer, where they would display as "NaN%".
        let nan = h(&[(H_5H_UTILIZATION, "NaN"), (H_7D_UTILIZATION, "0.2")]);
        assert!(parse_headers(&nan).is_none());

        let inf = h(&[(H_5H_UTILIZATION, "0.2"), (H_7D_UTILIZATION, "inf")]);
        assert!(parse_headers(&inf).is_none());
    }

    #[test]
    fn a_garbage_reset_header_does_not_lose_the_reading() {
        // The reading is the valuable part; a bad reset time degrades to None.
        let headers = h(&[
            (H_5H_UTILIZATION, "0.3"),
            (H_7D_UTILIZATION, "0.4"),
            (H_5H_RESET, "not-a-number"),
        ]);
        let s = parse_headers(&headers).expect("should still parse");
        assert_eq!(s.session_percent, 30.0);
        assert_eq!(s.session_reset_at_ms, None);
    }

    #[test]
    fn rounds_half_away_from_zero_like_swift() {
        // Swift's .rounded() is half-away-from-zero; f64::round matches.
        // 0.005 * 100 = 0.5 -> 1, not 0 (which banker's rounding would give).
        let headers = h(&[(H_5H_UTILIZATION, "0.005"), (H_7D_UTILIZATION, "0.015")]);
        let s = parse_headers(&headers).expect("should parse");
        assert_eq!(s.session_percent, 1.0);
        assert_eq!(s.weekly_percent, 2.0);
    }

    #[test]
    fn whitespace_is_tolerated() {
        let headers = h(&[(H_5H_UTILIZATION, " 0.43 "), (H_7D_UTILIZATION, "0.71")]);
        let s = parse_headers(&headers).expect("should parse");
        assert_eq!(s.session_percent, 43.0);
    }

    #[test]
    fn a_429_response_still_yields_a_reading() {
        // Documents the contract rather than exercising branching code: usage
        // headers ride along on 429s, so the caller must parse them anyway.
        let s = parse_headers(&full()).expect("429 headers parse like any other");
        assert_eq!(s.session_percent, 43.0);
    }

    #[test]
    fn full_and_empty_windows_parse() {
        let zero = h(&[(H_5H_UTILIZATION, "0"), (H_7D_UTILIZATION, "0")]);
        let s = parse_headers(&zero).expect("should parse");
        assert_eq!(s.session_percent, 0.0);

        let full_window = h(&[(H_5H_UTILIZATION, "1"), (H_7D_UTILIZATION, "1")]);
        let s = parse_headers(&full_window).expect("should parse");
        assert_eq!(s.session_percent, 100.0);
    }
}
