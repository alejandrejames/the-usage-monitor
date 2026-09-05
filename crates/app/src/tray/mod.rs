//! Tray icon: building it, and updating it from a snapshot.
//!
//! The per-platform split is deliberate and measured (docs/cross-platform.md):
//!
//! - **macOS** renders both rows into the icon, which is what the Swift app did.
//! - **Windows** does the same at the DPI-queried size; `set_title` is
//!   unsupported there, so the numbers must live in the bitmap. `set_tooltip`
//!   carries the detail.
//! - **Linux** puts the numbers in `set_title` and redraws the icon only when
//!   its colour bucket changes, because `set_icon` there writes a PNG to disk
//!   on every call — at a 60 s poll that would be ~1,440 writes a day. See the
//!   `linux` submodule.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod render;
pub mod sizing;

use crate::state::AppSnapshot;
use render::Row;
use tauri::image::Image;
use tauri::tray::TrayIcon;

/// Rows for the current snapshot, honouring the display preference.
fn rows_for(snapshot: &AppSnapshot) -> Vec<Row> {
    use crate::settings::TrayDisplay;

    // Only blank the tray when there is genuinely no reading. A stale poll
    // still has last-known numbers worth showing — at 100% usage that reading
    // is precisely what explains why requests are failing. The popover's
    // notice says it is not fresh.
    let disconnected = !has_reading(snapshot);
    let row = |percent: f64| {
        if disconnected {
            Row::disconnected()
        } else {
            Row::usage(percent)
        }
    };

    // Icons replace the SF Symbols the Swift app used (timer / calendar).
    use render::RowIcon;
    match crate::settings::get().tray_display {
        TrayDisplay::Session => vec![row(snapshot.session_percent).with_icon(RowIcon::Session)],
        TrayDisplay::Weekly => vec![row(snapshot.weekly_percent).with_icon(RowIcon::Weekly)],
        TrayDisplay::Both => vec![
            row(snapshot.session_percent).with_icon(RowIcon::Session),
            row(snapshot.weekly_percent).with_icon(RowIcon::Weekly),
        ],
    }
}

/// True when the snapshot holds a usable reading, fresh or not.
fn has_reading(snapshot: &AppSnapshot) -> bool {
    snapshot.is_authenticated && snapshot.last_updated_ms.is_some()
}

/// Human-readable summary for the tooltip. Linux has no tooltip support, so
/// this is unused there.
#[cfg(not(target_os = "linux"))]
fn tooltip_for(snapshot: &AppSnapshot) -> String {
    if !snapshot.is_authenticated {
        return "Claude Code not detected".into();
    }
    if snapshot.rate_limited {
        return format!(
            "Session limit reached — {}% used",
            snapshot.session_percent.round() as i64
        );
    }
    if snapshot.is_stale {
        return if has_reading(snapshot) {
            "Claude Usage — last known reading".into()
        } else {
            "Claude Usage — no connection".into()
        };
    }
    format!(
        "Session {}% · Weekly {}%",
        snapshot.session_percent.round() as i64,
        snapshot.weekly_percent.round() as i64
    )
}

/// Pushes the current snapshot to the tray.
pub fn update(tray: &TrayIcon, snapshot: &AppSnapshot) {
    let rows = rows_for(snapshot);

    #[cfg(target_os = "linux")]
    {
        use linux::IconBuckets;
        use std::sync::Mutex;

        // Numbers go in the title, which is cheap to set.
        let title = if !has_reading(snapshot) {
            "-- · --".to_string()
        } else {
            format!(
                "{}% · {}%",
                snapshot.session_percent.round() as i64,
                snapshot.weekly_percent.round() as i64
            )
        };
        let _ = tray.set_title(Some(title));
        // set_tooltip is unsupported on Linux; the detail lives in the menu.

        // The icon is only redrawn when its colour bucket changes. Every
        // set_icon here writes a PNG into $XDG_RUNTIME_DIR, so redrawing per
        // poll would mean ~1,440 writes a day. `set_title` also needs *an*
        // icon present to display at all, hence setting one at least once.
        static LAST_BUCKETS: Mutex<Option<IconBuckets>> = Mutex::new(None);

        let buckets = if !has_reading(snapshot) {
            IconBuckets::disconnected()
        } else {
            IconBuckets::of(Some(snapshot.session_percent), Some(snapshot.weekly_percent))
        };

        let mut last = LAST_BUCKETS.lock().expect("bucket lock");
        if last.as_ref() != Some(&buckets) {
            let rendered = render::render_at(&rows, sizing::tray_icon_height());
            let image = Image::new_owned(rendered.rgba, rendered.width, rendered.height);
            let _ = tray.set_icon(Some(image));
            *last = Some(buckets);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Windows reports the size it wants for the current DPI, and downscales
        // an oversized icon poorly; macOS scales cleanly from a fixed 3x buffer.
        // `sizing` resolves both. Queried per render, so moving between
        // monitors with different scaling re-renders at the right size.
        let rendered = render::render_at(&rows, sizing::tray_icon_height());
        let image = Image::new_owned(rendered.rgba, rendered.width, rendered.height);
        let _ = tray.set_icon(Some(image));

        // Colour carries the threshold, so the icon must not be templated —
        // template mode discards colour and keeps only alpha. `set_icon` also
        // resets this flag, so it is re-applied after every icon change.
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(false);

        let _ = tray.set_tooltip(Some(tooltip_for(snapshot)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> AppSnapshot {
        AppSnapshot {
            session_percent: 43.0,
            weekly_percent: 71.0,
            is_authenticated: true,
            // A reading only counts as one if a poll actually produced it.
            last_updated_ms: Some(1_788_000_000_000),
            ..Default::default()
        }
    }

    #[test]
    fn healthy_snapshots_show_percentages() {
        let rows = rows_for(&healthy());
        assert_eq!(rows[0].text, "43%");
        assert_eq!(rows[1].text, "71%");
    }

    #[test]
    fn a_stale_snapshot_keeps_its_last_reading() {
        // The regression this guards: blanking on staleness meant that at 100%
        // usage — when requests start failing — the tray went empty, hiding
        // the very number that explained why.
        let snapshot = AppSnapshot { is_stale: true, ..healthy() };
        let rows = rows_for(&snapshot);
        assert_eq!(rows[0].text, "43%");
        assert_eq!(rows[1].text, "71%");
    }

    #[test]
    fn only_a_reading_that_never_happened_is_blank() {
        let never_polled = AppSnapshot { last_updated_ms: None, ..healthy() };
        assert_eq!(rows_for(&never_polled)[0].text, "--");
    }

    #[test]
    fn a_rate_limited_snapshot_shows_its_numbers() {
        // 429 is a successful reading, not a failure: the quota is spent and
        // the percentage saying so is the whole point.
        let limited =
            AppSnapshot { rate_limited: true, session_percent: 100.0, ..healthy() };
        assert_eq!(rows_for(&limited)[0].text, "100%");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_tooltip_names_the_limit_case() {
        // "no connection" and "quota spent" look identical from the numbers,
        // so the tooltip has to distinguish them.
        let limited =
            AppSnapshot { rate_limited: true, session_percent: 100.0, ..healthy() };
        assert!(tooltip_for(&limited).contains("limit reached"));

        let stale = AppSnapshot { is_stale: true, ..healthy() };
        assert!(tooltip_for(&stale).contains("last known"));

        let never = AppSnapshot { is_stale: true, last_updated_ms: None, ..healthy() };
        assert!(tooltip_for(&never).contains("no connection"));
    }

    #[test]
    fn a_single_row_display_renders_at_full_height() {
        // A one-row icon must fill the tray height; sharing it with an absent
        // second row would render at half size and look shrunken.
        let one = render::render_at(&[Row::usage(43.0)], 54);
        let two = render::render_at(&[Row::usage(43.0), Row::usage(71.0)], 54);
        assert_eq!(one.height, two.height, "both fill the same icon box");
        assert!(one.width > two.width, "taller glyphs are also wider");
    }

    #[test]
    fn unauthenticated_snapshots_show_placeholders() {
        let rows = rows_for(&AppSnapshot::default());
        assert_eq!(rows[0].text, "--");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn the_tooltip_reads_normally_when_healthy() {
        assert_eq!(tooltip_for(&healthy()), "Session 43% · Weekly 71%");
        assert!(tooltip_for(&AppSnapshot::default()).contains("not detected"));
    }
}
