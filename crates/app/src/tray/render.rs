//! Rendering the tray icon: two stacked rows of "NN%", coloured by threshold.
//!
//! Replaces the `NSImage` drawing in `ClaudeUsageApp.swift`, which existed
//! because a macOS menu-bar item clips multiline text. The same constraint
//! applies everywhere, and on Windows there is no tray text at all, so every
//! platform gets its numbers this way.
//!
//! **The caller decides the height**; see the `sizing` module for why it
//! differs per platform. macOS scales an oversized buffer down cleanly, so it
//! renders at 3x for Retina sharpness; Windows downscales poorly and must be
//! given exactly the size the shell reports. See docs/cross-platform.md.

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

/// Row icons, bundled the same way as the font. These replace the SF Symbols
/// (`timer`, `calendar`) the Swift app used, which do not exist off-Apple.
const ICON_SESSION: &[u8] = include_bytes!("../../assets/icon-session.png");
const ICON_WEEKLY: &[u8] = include_bytes!("../../assets/icon-weekly.png");

/// Which icon a row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowIcon {
    Session,
    Weekly,
}

impl RowIcon {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Session => ICON_SESSION,
            Self::Weekly => ICON_WEEKLY,
        }
    }
}

/// A decoded icon: straight RGBA at its source resolution.
struct DecodedIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Decodes a bundled PNG. Returns `None` rather than panicking so a bad asset
/// degrades to a text-only icon instead of taking the app down.
fn decode(bytes: &[u8]) -> Option<DecodedIcon> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    // The bundled icons are RGBA8; anything else is not something to guess at.
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    Some(DecodedIcon { rgba: buf, width: info.width, height: info.height })
}

/// Nearest-neighbour box sample into `size` x `size`, compositing onto `dst`.
///
/// Box-averaging rather than a single sample: these icons shrink from 1024px to
/// roughly 20, and point-sampling that far down drops most strokes entirely.
fn blit_icon(
    icon: &DecodedIcon,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    at_x: i32,
    at_y: i32,
    size: u32,
) {
    if size == 0 {
        return;
    }
    let step_x = icon.width as f32 / size as f32;
    let step_y = icon.height as f32 / size as f32;

    for oy in 0..size {
        for ox in 0..size {
            // Average the source block this destination pixel covers.
            let (x0, x1) = ((ox as f32 * step_x) as u32, (((ox + 1) as f32) * step_x) as u32);
            let (y0, y1) = ((oy as f32 * step_y) as u32, (((oy + 1) as f32) * step_y) as u32);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);

            for sy in y0..y1.max(y0 + 1).min(icon.height) {
                for sx in x0..x1.max(x0 + 1).min(icon.width) {
                    let i = ((sy * icon.width + sx) * 4) as usize;
                    if i + 3 >= icon.rgba.len() {
                        continue;
                    }
                    r += icon.rgba[i] as u32;
                    g += icon.rgba[i + 1] as u32;
                    b += icon.rgba[i + 2] as u32;
                    a += icon.rgba[i + 3] as u32;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let (r, g, b, a) = ((r / n) as u8, (g / n) as u8, (b / n) as u8, (a / n) as u8);
            if a == 0 {
                continue;
            }

            let (dx, dy) = (at_x + ox as i32, at_y + oy as i32);
            if dx < 0 || dy < 0 || dx >= dst_w as i32 || dy >= dst_h as i32 {
                continue;
            }
            let di = ((dy as u32 * dst_w + dx as u32) * 4) as usize;
            // Source-over onto whatever is already there.
            let sa = a as u32;
            let inv = 255 - sa;
            dst[di] = ((r as u32 * sa + dst[di] as u32 * inv) / 255) as u8;
            dst[di + 1] = ((g as u32 * sa + dst[di + 1] as u32 * inv) / 255) as u8;
            dst[di + 2] = ((b as u32 * sa + dst[di + 2] as u32 * inv) / 255) as u8;
            dst[di + 3] = dst[di + 3].max(a);
        }
    }
}

/// A single row of the tray icon.
pub struct Row {
    pub text: String,
    pub rgb: [u8; 3],
    /// Drawn to the left of the text, if any.
    pub icon: Option<RowIcon>,
}

impl Row {
    /// A usage row: the percentage, coloured by its threshold bucket.
    pub fn usage(percent: f64) -> Self {
        Self {
            text: format!("{}%", percent.round() as i64),
            rgb: UsageLevel::for_percent(percent).rgb(),
            icon: None,
        }
    }

    /// The same row with an icon beside it.
    pub fn with_icon(mut self, icon: RowIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The disconnected placeholder.
    ///
    /// The Swift version swapped in a `wifi.slash` SF Symbol, which does not
    /// exist off-Apple and would be illegible at 16 px on Windows anyway. A
    /// dash in secondary grey reads correctly at every size.
    pub fn disconnected() -> Self {
        Self { text: "--".into(), rgb: [0x8E, 0x8E, 0x93], icon: None }
    }
}

/// An RGBA image destined for the tray.
pub struct Rendered {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Renders into a buffer of exactly `total_h` pixels tall.
///
/// Width follows from the widest row, so the icon keeps the text's aspect
/// ratio rather than being forced square.
pub fn render_at(rows: &[Row], total_h: u32) -> Rendered {
    let font = FontRef::try_from_slice(FONT).expect("bundled font must parse");

    // One row fills the icon; two share it. Without this a single-row display
    // would render at half height and look shrunken next to other menu items.
    let rows_count = rows.len().max(1) as f32;
    let gap = if rows_count > 1.0 { (total_h as f32 * 0.02).max(1.0) } else { 0.0 };
    let row_h = (total_h as f32 - gap) / rows_count;
    // Two rows can overshoot their box — digits have no descenders, so the
    // glyphs grow into space the metrics reserve but never use. A single row
    // already owns the full height, and the same overshoot would make the icon
    // several times wider than the menu bar wants.
    let text_ratio = if rows_count > 1.0 { 1.18 } else { 0.82 };
    let scale = PxScale::from(row_h * text_ratio);
    let scaled = font.as_scaled(scale);

    // Icons are square, sized to the row and inset slightly so they sit
    // optically level with the digits rather than overpowering them.
    let icon_size = (row_h * 0.70).round().max(1.0);
    let icon_gap = (icon_size * 0.10).round().max(1.0);
    let any_icons = rows.iter().any(|r| r.icon.is_some());
    let text_offset = if any_icons { icon_size + icon_gap } else { 0.0 };

    // Width comes from the widest row, so a jump to "100%" does not clip. The
    // font's uniform digit advance keeps this stable across polls.
    let text_width = rows
        .iter()
        .map(|r| r.text.chars().map(|c| scaled.h_advance(font.glyph_id(c))).sum::<f32>())
        .fold(0.0f32, f32::max);
    let width = ((text_offset + text_width).ceil() as u32 + 2).max(1);

    let mut rgba = vec![0u8; (width * total_h * 4) as usize];

    // Decoded once per render rather than per row — both rows may want one.
    let session_icon = any_icons.then(|| decode(RowIcon::Session.bytes())).flatten();
    let weekly_icon = any_icons.then(|| decode(RowIcon::Weekly.bytes())).flatten();

    for (i, row) in rows.iter().enumerate() {
        let row_top = gap / 2.0 + row_h * i as f32;
        // Baseline sits slightly above the row's bottom edge to leave room for
        // descenders; '%' has none but the metric keeps rows optically even.
        let baseline_y = gap / 2.0 + row_h * (i as f32 + 1.0) - row_h * 0.08;

        if let Some(kind) = row.icon {
            let decoded = match kind {
                RowIcon::Session => session_icon.as_ref(),
                RowIcon::Weekly => weekly_icon.as_ref(),
            };
            if let Some(decoded) = decoded {
                blit_icon(
                    decoded,
                    &mut rgba,
                    width,
                    total_h,
                    1,
                    (row_top + (row_h - icon_size) / 2.0).round() as i32,
                    icon_size as u32,
                );
            }
        }

        let mut pen_x = 1.0 + text_offset;

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
    use crate::tray::sizing::tray_icon_height;

    /// Renders at the platform's current tray height.
    fn render_at_default(rows: &[Row]) -> Rendered {
        render_at(rows, tray_icon_height())
    }

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
    fn the_bundled_row_icons_decode() {
        // decode() returns None on a bad asset so the tray degrades to text
        // rather than panicking — which means a broken icon would otherwise
        // ship silently.
        for kind in [RowIcon::Session, RowIcon::Weekly] {
            let icon = decode(kind.bytes()).unwrap_or_else(|| panic!("{kind:?} failed to decode"));
            assert!(icon.width > 0 && icon.height > 0);
            assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        }
    }

    #[test]
    fn an_icon_widens_the_row() {
        let plain = render_at(&[Row::usage(72.0)], 54);
        let with_icon = render_at(&[Row::usage(72.0).with_icon(RowIcon::Session)], 54);
        assert!(with_icon.width > plain.width, "the icon must reserve its own space");
    }

    #[test]
    fn the_icon_stays_a_reasonable_shape() {
        // The menu bar fixes the height, so width is the only cost — an icon
        // that grows sideways eats the user's menu bar. These bounds are wide
        // enough for "100%" in both rows but catch a runaway scale factor.
        let two = render_at(
            &[
                Row::usage(90.0).with_icon(RowIcon::Session),
                Row::usage(63.0).with_icon(RowIcon::Weekly),
            ],
            44,
        );
        let ratio = two.width as f32 / two.height as f32;
        assert!(ratio < 2.2, "two-row icon is {ratio:.2}x wide, too much menu bar");

        // One row owns the full height, so it is inherently wider — but not
        // unboundedly so.
        let one = render_at(&[Row::usage(90.0).with_icon(RowIcon::Session)], 44);
        let ratio = one.width as f32 / one.height as f32;
        assert!(ratio < 3.0, "single-row icon is {ratio:.2}x wide, too much menu bar");
    }

    #[test]
    fn renders_a_correctly_sized_buffer() {
        let r = render_at_default(&[Row::usage(43.0), Row::usage(71.0)]);
        assert_eq!(r.height, tray_icon_height());
        assert_eq!(r.rgba.len(), (r.width * r.height * 4) as usize);
        assert!(r.width > 0);
    }

    #[test]
    fn width_is_stable_across_equal_digit_counts() {
        // 43%/71% and 88%/12% are both two digits, so the icon must not
        // resize as usage changes.
        let a = render_at_default(&[Row::usage(43.0), Row::usage(71.0)]);
        let b = render_at_default(&[Row::usage(88.0), Row::usage(12.0)]);
        assert_eq!(a.width, b.width);
    }

    #[test]
    fn a_hundred_percent_widens_the_icon() {
        // Three digits genuinely need more room; the icon must grow rather
        // than clip.
        let two = render_at_default(&[Row::usage(43.0), Row::usage(71.0)]);
        let three = render_at_default(&[Row::usage(100.0), Row::usage(100.0)]);
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
        let r = render_at_default(&[Row::usage(43.0), Row::usage(71.0)]);
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
        let r = render_at_default(&[Row::disconnected(), Row::disconnected()]);
        assert!(r.rgba.chunks(4).any(|px| px[3] > 0));
    }
}
