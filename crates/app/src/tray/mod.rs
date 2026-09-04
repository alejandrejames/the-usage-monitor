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

/// Rows for the current snapshot.
fn rows_for(snapshot: &AppSnapshot) -> Vec<Row> {
    if snapshot.is_stale || !snapshot.is_authenticated {
        vec![Row::disconnected(), Row::disconnected()]
    } else {
        vec![Row::usage(snapshot.session_percent), Row::usage(snapshot.weekly_percent)]
    }
}

/// Human-readable summary for the tooltip. Linux has no tooltip support, so
/// this is unused there.
#[cfg(not(target_os = "linux"))]
fn tooltip_for(snapshot: &AppSnapshot) -> String {
    if !snapshot.is_authenticated {
        return "Claude Code not detected".into();
    }
    if snapshot.is_stale {
        return "Claude Usage — no connection".into();
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
        let title = if snapshot.is_stale || !snapshot.is_authenticated {
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

        let buckets = if snapshot.is_stale || !snapshot.is_authenticated {
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
    fn stale_snapshots_show_placeholders() {
        // A faded percentage would read as a real (low) value, which is why
        // the Swift version swapped the glyph rather than dimming the number.
        let snapshot = AppSnapshot { is_stale: true, ..healthy() };
        let rows = rows_for(&snapshot);
        assert_eq!(rows[0].text, "--");
        assert_eq!(rows[1].text, "--");
    }

    #[test]
    fn unauthenticated_snapshots_show_placeholders() {
        let rows = rows_for(&AppSnapshot::default());
        assert_eq!(rows[0].text, "--");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn tooltip_distinguishes_the_three_states() {
        assert_eq!(tooltip_for(&healthy()), "Session 43% · Weekly 71%");
        assert!(tooltip_for(&AppSnapshot { is_stale: true, ..healthy() }).contains("no connection"));
        assert!(tooltip_for(&AppSnapshot::default()).contains("not detected"));
    }
}
