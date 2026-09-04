//! Claude service status, parsed from status.claude.com's public Statuspage API.

use crate::model::{ServiceHealth, ServiceStatus};

/// The unauthenticated summary endpoint — the same JSON the public status page
/// renders from.
pub const ENDPOINT: &str = "https://status.claude.com/api/v2/summary.json";

/// Status changes far more slowly than usage, and this keeps us well clear of
/// any rate limiting on the public status API.
pub const POLL_INTERVAL_SECS: u64 = 300;

/// The components we surface, in display order. Matched by Statuspage component
/// id (stable) with a name fallback, so a cosmetic rename upstream does not
/// blank the row.
pub const TRACKED: &[(&str, &str)] =
    &[("rwppv331jlwc", "claude.ai"), ("yyzkbfz2thpt", "Claude Code")];

/// Parses the summary payload into the tracked rows.
///
/// Deliberately loose, mirroring the Swift version's use of `JSONSerialization`
/// over `Codable`: unrelated parts of the feed (incident bodies, new fields)
/// must never fail the decode of the few keys we actually read. Returns `None`
/// only when the payload has no `components` array at all.
pub fn parse_summary(body: &str) -> Option<Vec<ServiceStatus>> {
    let root: serde_json::Value = serde_json::from_str(body).ok()?;
    let components = root.get("components")?.as_array()?;

    Some(
        TRACKED
            .iter()
            .map(|(want_id, want_name)| {
                // Prefer the stable id; fall back to the display name.
                let found = components
                    .iter()
                    .find(|c| c.get("id").and_then(|v| v.as_str()) == Some(want_id))
                    .or_else(|| {
                        components
                            .iter()
                            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(want_name))
                    });

                let name =
                    found.and_then(|c| c.get("name")).and_then(|v| v.as_str()).unwrap_or(want_name);

                let health = found
                    .and_then(|c| c.get("status"))
                    .and_then(|v| v.as_str())
                    .map(ServiceHealth::from_api)
                    .unwrap_or(ServiceHealth::Unknown);

                ServiceStatus { id: (*want_id).to_string(), name: name.to_string(), health }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{all_operational, overall_health};

    fn summary(components: &str) -> String {
        format!(r#"{{"page":{{"id":"x"}},"components":[{components}]}}"#)
    }

    #[test]
    fn parses_both_tracked_components_by_id() {
        let body = summary(
            r#"
            {"id":"rwppv331jlwc","name":"claude.ai","status":"operational"},
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"degraded_performance"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "claude.ai");
        assert_eq!(services[0].health, ServiceHealth::Operational);
        assert_eq!(services[1].name, "Claude Code");
        assert_eq!(services[1].health, ServiceHealth::Degraded);
    }

    #[test]
    fn preserves_tracked_display_order() {
        // Upstream order must not leak through — claude.ai is always first.
        let body = summary(
            r#"
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"operational"},
            {"id":"rwppv331jlwc","name":"claude.ai","status":"operational"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services[0].id, "rwppv331jlwc");
        assert_eq!(services[1].id, "yyzkbfz2thpt");
    }

    #[test]
    fn falls_back_to_name_when_the_id_changes() {
        // The whole reason the fallback exists: an id churn upstream must not
        // blank the row.
        let body = summary(
            r#"
            {"id":"NEW-ID-9999","name":"claude.ai","status":"major_outage"},
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"operational"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services[0].health, ServiceHealth::MajorOutage);
    }

    #[test]
    fn id_match_wins_over_name_match() {
        // If both are present, the stable id is authoritative.
        let body = summary(
            r#"
            {"id":"decoy","name":"claude.ai","status":"major_outage"},
            {"id":"rwppv331jlwc","name":"Claude (renamed)","status":"operational"},
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"operational"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services[0].health, ServiceHealth::Operational);
        // And the upstream rename is reflected in the label.
        assert_eq!(services[0].name, "Claude (renamed)");
    }

    #[test]
    fn a_missing_component_is_unknown_not_an_error() {
        let body = summary(r#"{"id":"rwppv331jlwc","name":"claude.ai","status":"operational"}"#);
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services.len(), 2);
        assert_eq!(services[1].health, ServiceHealth::Unknown);
        // The fallback label keeps the row readable.
        assert_eq!(services[1].name, "Claude Code");
    }

    #[test]
    fn unrelated_fields_do_not_break_the_parse() {
        // Incident bodies and new keys appear in this feed constantly.
        let body = r#"{
            "page": {"id":"x","updated_at":"2026-09-04T00:00:00Z"},
            "incidents": [{"id":"i1","name":"Something","shortlink":"http://x"}],
            "scheduled_maintenances": [],
            "some_new_field": {"nested": [1,2,3]},
            "components": [
                {"id":"rwppv331jlwc","name":"claude.ai","status":"operational","brand_new":true},
                {"id":"yyzkbfz2thpt","name":"Claude Code","status":"operational"}
            ],
            "status": {"indicator":"none"}
        }"#;
        let services = parse_summary(body).expect("should parse");
        assert!(all_operational(&services));
    }

    #[test]
    fn malformed_payloads_yield_none() {
        assert!(parse_summary("not json").is_none());
        assert!(parse_summary(r#"{"page":{}}"#).is_none());
        assert!(parse_summary(r#"{"components":"not an array"}"#).is_none());
    }

    #[test]
    fn an_empty_component_list_still_yields_unknown_rows() {
        let services = parse_summary(r#"{"components":[]}"#).expect("should parse");
        assert_eq!(services.len(), 2);
        assert!(services.iter().all(|s| s.health == ServiceHealth::Unknown));
        assert!(!all_operational(&services));
    }

    #[test]
    fn an_unrecognised_status_string_is_unknown() {
        let body = summary(
            r#"
            {"id":"rwppv331jlwc","name":"claude.ai","status":"invented_status"},
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"operational"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(services[0].health, ServiceHealth::Unknown);
    }

    #[test]
    fn overall_reflects_the_worst_tracked_component() {
        let body = summary(
            r#"
            {"id":"rwppv331jlwc","name":"claude.ai","status":"operational"},
            {"id":"yyzkbfz2thpt","name":"Claude Code","status":"partial_outage"}
            "#,
        );
        let services = parse_summary(&body).expect("should parse");
        assert_eq!(overall_health(&services), ServiceHealth::PartialOutage);
        assert!(!all_operational(&services));
    }
}
