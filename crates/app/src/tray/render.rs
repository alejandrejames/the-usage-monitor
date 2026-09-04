//! Rendering the tray icon: two stacked rows of "NN%", coloured by threshold.
//!
//! Replaces the `NSImage` drawing in `ClaudeUsageApp.swift`, which existed
//! because a macOS menu-bar item clips multiline text. The same constraint
//! applies everywhere, and on Windows there is no tray text at all, so every
//! platform gets its numbers this way.
//!
//! **Sizing is by aspect ratio, not absolute pixels.** Spike B established that
//! the macOS backend normalises the icon to a fixed height in *points* and
//! derives width from the source aspect ratio, so a 2x buffer is scaled down
//! rather than clipped. Supplying a larger buffer therefore costs nothing and
//! buys Retina sharpness. See docs/cross-platform.md.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use claudeusage_core::UsageLevel;

/// DejaVu Sans Bold, bundled rather than loaded from the system.
///
/// Two reasons this font specifically: its licence permits redistribution, and
/// **all ten digits share one advance width** (verified: 19.127518 at 32 px),
/// so "43%" and "100%" do not make the icon jitter between polls. System font
/// enumeration would also differ per platform and is unavailable in some Linux
/// containers. `ab_glyph` cannot parse `.ttc` collections, so this must stay a
/// plain `.ttf`.
const FONT: &[u8] = include_bytes!("../../assets/DejaVuSans-Bold.ttf");

/// Rendered at 3x the macOS 18 pt tray height, giving a crisp source for both
/// 1x and 2x displays after the backend scales it down.
const RENDER_HEIGHT: u32 = 54;

/// A single row of the tray icon.
pub struct Row {
    pub text: String,
    pub rgb: [u8; 3],
}

impl Row {
    /// A usage row: the percentage, coloured by its threshold bucket.
    pub fn usage(percent: f64) -> Self {
        Self {
            text: format!("{}%", percent.round() as i64),
            rgb: UsageLevel::for_percent(percent).rgb(),
        }
    }

    /// The disconnected placeholder.
    ///
    /// The Swift version swapped in a `wifi.slash` SF Symbol, which does not
    /// exist off-Apple and would be illegible at 16 px on Windows anyway. A
    /// dash in secondary grey reads correctly at every size.
    pub fn disconnected() -> Self {
        Self { text: "--".into(), rgb: [0x8E, 0x8E, 0x93] }
    }
}

/// An RGBA image destined for the tray.
pub struct Rendered {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Renders the two-row icon.
pub fn stacked(rows: &[Row]) -> Rendered {
    render_at(rows, RENDER_HEIGHT)
}

/// Renders into a buffer of exactly `total_h` pixels tall.
///
/// Windows needs this directly: its tray icon size is fixed by DPI
/// (`GetSystemMetrics(SM_CXSMICON)` gives 16/20/24/32) rather than scaled from
/// an aspect ratio, so the caller passes the queried size.
pub fn render_at(rows: &[Row], total_h: u32) -> Rendered {
    let font = FontRef::try_from_slice(FONT).expect("bundled font must parse");

    let gap = (total_h as f32 * 0.06).max(1.0);
    let row_h = (total_h as f32 - gap) / 2.0;
    let scale = PxScale::from(row_h * 1.05);
    let scaled = font.as_scaled(scale);

    // Width comes from the widest row, so a jump to "100%" does not clip. The
    // font's uniform digit advance keeps this stable across polls.
    let width = rows
        .iter()
        .map(|r| r.text.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum::<f32>())
        .fold(0.0f32, f32::max)
        .ceil() as u32
        + 2;
    let width = width.max(1);

    let mut rgba = vec![0u8; (width * total_h * 4) as usize];

    for (i, row) in rows.iter().enumerate() {
        // Baseline sits slightly above the row's bottom edge to leave room for
        // descenders; '%' has none but the metric keeps rows optically even.
        let baseline_y = gap / 2.0 + row_h * (i as f32 + 1.0) - row_h * 0.22;
        let mut pen_x = 1.0f32;

        for ch in row.text.chars() {
            let glyph_id = font.glyph_id(ch);
            let positioned =
                glyph_id.with_scale_and_position(scale, ab_glyph::point(pen_x, baseline_y));

            if let Some(outlined) = font.outline_glyph(positioned) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let x = bounds.min.x as i32 + gx as i32;
                    let y = bounds.min.y as i32 + gy as i32;
                    if x < 0 || y < 0 || x >= width as i32 || y >= total_h as i32 {
                        return;
                    }
                    let idx = ((y as u32 * width + x as u32) * 4) as usize;
                    let alpha = (coverage * 255.0) as u8;
                    // Keep the strongest coverage where glyphs overlap, so
                    // antialiased edges never punch holes in each other.
                    if alpha > rgba[idx + 3] {
                        rgba[idx] = row.rgb[0];
                        rgba[idx + 1] = row.rgb[1];
                        rgba[idx + 2] = row.rgb[2];
                        rgba[idx + 3] = alpha;
                    }
                });
            }
            pen_x += scaled.h_advance(glyph_id);
        }
    }

    Rendered { rgba, width, height: total_h }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_font_parses() {
        assert!(FontRef::try_from_slice(FONT).is_ok());
    }

    #[test]
    fn digits_are_tabular() {
        // The property the whole font choice rests on: if digit advances
        // differed, the icon width would change between polls and the tray
        // would visibly jitter.
        let font = FontRef::try_from_slice(FONT).unwrap();
        let scaled = font.as_scaled(PxScale::from(32.0));
        let widths: Vec<f32> =
            "0123456789".chars().map(|c| scaled.h_advance(font.glyph_id(c))).collect();
        assert!(
            widths.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
            "digit advances must be uniform, got {widths:?}"
        );
    }

    #[test]
    fn renders_a_correctly_sized_buffer() {
        let r = stacked(&[Row::usage(43.0), Row::usage(71.0)]);
        assert_eq!(r.height, RENDER_HEIGHT);
        assert_eq!(r.rgba.len(), (r.width * r.height * 4) as usize);
        assert!(r.width > 0);
    }

    #[test]
    fn width_is_stable_across_equal_digit_counts() {
        // 43%/71% and 88%/12% are both two digits, so the icon must not
        // resize as usage changes.
        let a = stacked(&[Row::usage(43.0), Row::usage(71.0)]);
        let b = stacked(&[Row::usage(88.0), Row::usage(12.0)]);
        assert_eq!(a.width, b.width);
    }

    #[test]
    fn a_hundred_percent_widens_the_icon() {
        // Three digits genuinely need more room; the icon must grow rather
        // than clip.
        let two = stacked(&[Row::usage(43.0), Row::usage(71.0)]);
        let three = stacked(&[Row::usage(100.0), Row::usage(100.0)]);
        assert!(three.width > two.width);
    }

    #[test]
    fn rows_use_their_threshold_colour() {
        assert_eq!(Row::usage(10.0).rgb, UsageLevel::Green.rgb());
        assert_eq!(Row::usage(75.0).rgb, UsageLevel::Amber.rgb());
        assert_eq!(Row::usage(95.0).rgb, UsageLevel::Red.rgb());
    }

    #[test]
    fn something_is_actually_drawn() {
        // Guards against a silent all-transparent render, which would show as
        // a blank tray rather than an error.
        let r = stacked(&[Row::usage(43.0), Row::usage(71.0)]);
        assert!(r.rgba.chunks(4).any(|px| px[3] > 0), "no opaque pixels rendered");
    }

    #[test]
    fn renders_at_windows_tray_sizes() {
        // 16px at 100% DPI is the cramped case the plan accepted; it must
        // still produce a valid buffer rather than panic or come out empty.
        for size in [16u32, 20, 24, 32] {
            let r = render_at(&[Row::usage(43.0), Row::usage(71.0)], size);
            assert_eq!(r.height, size);
            assert_eq!(r.rgba.len(), (r.width * r.height * 4) as usize);
            assert!(r.rgba.chunks(4).any(|px| px[3] > 0), "nothing drawn at {size}px");
        }
    }

    #[test]
    fn disconnected_rows_render() {
        let r = stacked(&[Row::disconnected(), Row::disconnected()]);
        assert!(r.rgba.chunks(4).any(|px| px[3] > 0));
    }
}
