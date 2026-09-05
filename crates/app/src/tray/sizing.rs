//! How large to render the tray icon, per platform.
//!
//! The two desktop platforms want opposite things, which is why this is not a
//! single constant:
//!
//! - **macOS** normalises the icon to a fixed height in *points* and derives
//!   width from the aspect ratio, so an oversized buffer is scaled down cleanly.
//!   Supplying 3x costs nothing and buys Retina sharpness.
//! - **Windows** asks for an exact pixel size and, per Microsoft's guidance,
//!   an icon that is too large "is subject to being downscaled (also poorly) by
//!   the OS". So the icon must match `SM_CXSMICON` for the current DPI.
//!
//! See docs/cross-platform.md.

/// macOS: 6x the 18 pt tray height the backend normalises to.
///
/// The backend scales this down to 18 pt regardless, so the number only sets
/// the source resolution — a larger buffer buys sharpness, not size. It is
/// doubled from the original 3x so the glyphs and row icons survive the
/// downscale with more detail.
#[cfg(not(windows))]
pub const DEFAULT_RENDER_HEIGHT: u32 = 108;

/// Windows fallback when the metric cannot be read: 16 px is the 100 % DPI size.
#[cfg(windows)]
const FALLBACK_ICON_SIZE: u32 = 16;

/// The height, in pixels, to render the tray icon at right now.
///
/// On Windows this reflects the current DPI, so it changes when the user moves
/// the app between monitors or changes scaling; call it per render rather than
/// caching the result.
#[cfg(windows)]
pub fn tray_icon_height() -> u32 {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
    use windows_sys::Win32::UI::WindowsAndMessaging::SM_CYSMICON;

    // SAFETY: both calls are simple metric reads with no pointer arguments.
    // GetSystemMetricsForDpi needs Windows 10 1607+, which is below Tauri v2's
    // own floor, so it is always present.
    let size = unsafe {
        let dpi = GetDpiForSystem();
        GetSystemMetricsForDpi(SM_CYSMICON, dpi)
    };

    // The metric returns 0 on failure.
    if size <= 0 {
        FALLBACK_ICON_SIZE
    } else {
        size as u32
    }
}

/// Non-Windows platforms render at a fixed height and let the OS scale.
#[cfg(not(windows))]
pub fn tray_icon_height() -> u32 {
    DEFAULT_RENDER_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_height_is_usable() {
        // Guards the Windows fallback path as much as the macOS constant: a
        // zero or absurd height would produce an invisible or broken icon.
        let h = tray_icon_height();
        assert!(h >= 16, "tray height {h} is too small to render two rows");
        assert!(h <= 256, "tray height {h} is implausibly large");
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_uses_the_fixed_height() {
        assert_eq!(tray_icon_height(), DEFAULT_RENDER_HEIGHT);
    }
}
