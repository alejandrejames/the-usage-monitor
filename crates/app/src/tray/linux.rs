//! Linux-specific tray concerns.
//!
//! Two things differ enough from the other platforms to live here.
//!
//! **The icon is not re-rendered per poll.** The GTK/AppIndicator backend takes
//! an `icon_path`, not a pixel buffer, so every `set_icon` writes a PNG into
//! `$XDG_RUNTIME_DIR`. At a 60-second poll that would be ~1,440 writes a day.
//! The numbers go in `set_title` instead, and the icon is only re-rendered when
//! the colour bucket actually changes — a handful of times a day.
//!
//! **The tray may not exist at all.** GNOME dropped StatusNotifierItem support
//! in 3.26 and never restored it; the ecosystem answer is the AppIndicator
//! extension, which every tray app on GNOME depends on. Rather than building a
//! bespoke fallback window, detect whether anything is listening and tell the
//! user what to install. Bazzite ships GNOME, so this is the default experience
//! there.
//!
//! See docs/cross-platform.md.

use claudeusage_core::UsageLevel;

/// The colour buckets currently shown in the tray, so a redraw only happens
/// when one actually changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconBuckets {
    pub session: Option<UsageLevel>,
    pub weekly: Option<UsageLevel>,
}

impl IconBuckets {
    /// Buckets for a snapshot. `None` means the disconnected placeholder, which
    /// is its own visual state and must not be confused with a usage colour.
    pub fn of(session: Option<f64>, weekly: Option<f64>) -> Self {
        Self {
            session: session.map(UsageLevel::for_percent),
            weekly: weekly.map(UsageLevel::for_percent),
        }
    }

    /// Disconnected in both rows.
    pub fn disconnected() -> Self {
        Self { session: None, weekly: None }
    }
}

/// Whether a tray host is present on the session bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayHost {
    /// A StatusNotifierWatcher is registered; the tray will work.
    Present,
    /// Nothing is listening — on GNOME this means the AppIndicator extension
    /// is missing.
    Absent,
    /// Could not tell (no D-Bus tooling available). Treated as present, since
    /// warning on a false negative is worse than staying quiet.
    Unknown,
}

/// The well-known bus name a tray host registers.
const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";

/// Checks whether a StatusNotifierWatcher is on the session bus.
///
/// Uses `gdbus`, which ships with GLib and is therefore present wherever
/// GTK is — no extra dependency for a check that runs once at startup.
///
/// Registration can race with the shell's own startup, so a caller that
/// wants certainty should retry; a single negative at launch is not proof
/// the tray is unavailable.
pub fn detect_tray_host() -> TrayHost {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            WATCHER_NAME,
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            // gdbus prints a GVariant tuple: "(true,)" or "(false,)".
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("true") {
                TrayHost::Present
            } else {
                TrayHost::Absent
            }
        }
        // No gdbus, or no session bus to talk to.
        _ => TrayHost::Unknown,
    }
}

/// Advice to show when no tray host is found.
///
/// Names the command for the detected distro family rather than a generic
/// "install an extension", since the package name differs across the targets
/// this project supports.
pub fn missing_tray_advice() -> &'static str {
    advice_for(current_family())
}

fn advice_for(family: Family) -> &'static str {
    if family == Family::ImmutableFedora {
        // Bazzite and other rpm-ostree systems: layering needs a reboot, and
        // the extension is usually better installed from the GNOME site.
        "No system tray found. On Bazzite/Silverblue, install the \
         'AppIndicator and KStatusNotifierItem Support' GNOME extension from \
         extensions.gnome.org, then log out and back in."
    } else if family == Family::DebianLike {
        "No system tray found. Install the AppIndicator extension:\n  \
         sudo apt install gnome-shell-extension-appindicator\n\
         then log out and back in."
    } else if family == Family::ArchLike {
        "No system tray found. Install the AppIndicator extension:\n  \
         sudo pacman -S libappindicator-gtk3 gnome-shell-extension-appindicator\n\
         then log out and back in."
    } else {
        "No system tray found. On GNOME, install the 'AppIndicator and \
         KStatusNotifierItem Support' extension from extensions.gnome.org, \
         then log out and back in."
    }
}

/// Pulls one field out of `/etc/os-release` content. Split from the file read
/// so the distro predicates can be tested against real os-release samples from
/// machines this build cannot run on.
fn parse_os_release(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(value) = line.strip_prefix(&format!("{key}=")) {
            return Some(value.trim_matches('"').to_ascii_lowercase());
        }
    }
    None
}

/// Distro family, derived from os-release content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    ImmutableFedora,
    DebianLike,
    ArchLike,
    Other,
}

fn family_from(content: &str) -> Family {
    let id = parse_os_release(content, "ID").unwrap_or_default();
    let like = parse_os_release(content, "ID_LIKE").unwrap_or_default();
    let variant = parse_os_release(content, "VARIANT_ID").unwrap_or_default();

    if id.contains("bazzite")
        || variant.contains("silverblue")
        || variant.contains("kinoite")
        || variant.contains("ostree")
    {
        Family::ImmutableFedora
    } else if id.contains("debian") || id.contains("ubuntu") || like.contains("debian") {
        Family::DebianLike
    } else if id.contains("arch") || like.contains("arch") {
        Family::ArchLike
    } else {
        Family::Other
    }
}

/// This machine's distro family.
fn current_family() -> Family {
    std::fs::read_to_string("/etc/os-release").map(|c| family_from(&c)).unwrap_or(Family::Other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_change_only_when_the_colour_does() {
        // The whole point: 43% and 55% are both green, so no redraw — which is
        // what keeps Linux from writing a PNG on every poll.
        let a = IconBuckets::of(Some(43.0), Some(71.0));
        let b = IconBuckets::of(Some(55.0), Some(75.0));
        assert_eq!(a, b);

        // Crossing into amber is a real change.
        let c = IconBuckets::of(Some(72.0), Some(71.0));
        assert_ne!(a, c);
    }

    #[test]
    fn disconnected_is_distinct_from_every_usage_colour() {
        let disconnected = IconBuckets::disconnected();
        for percent in [0.0, 50.0, 75.0, 95.0, 100.0] {
            assert_ne!(
                disconnected,
                IconBuckets::of(Some(percent), Some(percent)),
                "{percent}% must not look like the disconnected state"
            );
        }
    }

    #[test]
    fn a_partial_reading_still_differs_from_a_full_one() {
        assert_ne!(IconBuckets::of(Some(50.0), None), IconBuckets::of(Some(50.0), Some(50.0)));
    }

    #[test]
    fn detection_never_panics() {
        // Runs on any host, including CI containers with no session bus. The
        // contract is that it returns something rather than failing.
        let host = detect_tray_host();
        assert!(matches!(host, TrayHost::Present | TrayHost::Absent | TrayHost::Unknown));
    }

    #[test]
    fn bazzite_is_recognised_as_immutable() {
        // Bazzite is a listed target and ships GNOME, so it is the most likely
        // machine to hit the missing-tray path. Its advice must not tell the
        // user to run a package manager that needs a reboot there.
        let bazzite = r#"NAME="Bazzite"
ID=bazzite
ID_LIKE="fedora"
VARIANT_ID=bazzite
"#;
        assert_eq!(family_from(bazzite), Family::ImmutableFedora);
        let advice = advice_for(Family::ImmutableFedora);
        assert!(advice.contains("Bazzite"));
        assert!(!advice.contains("apt"), "must not suggest apt on an rpm-ostree system");
        assert!(!advice.contains("pacman"));
    }

    #[test]
    fn silverblue_and_kinoite_are_immutable() {
        for variant in ["silverblue", "kinoite"] {
            let content = format!("ID=fedora\nVARIANT_ID={variant}\n");
            assert_eq!(family_from(&content), Family::ImmutableFedora, "{variant}");
        }
    }

    #[test]
    fn the_supported_distros_map_to_their_package_managers() {
        let cases = [
            ("ID=ubuntu\nID_LIKE=debian\n", Family::DebianLike, "apt"),
            ("ID=debian\n", Family::DebianLike, "apt"),
            ("ID=arch\n", Family::ArchLike, "pacman"),
            ("ID=cachyos\nID_LIKE=arch\n", Family::ArchLike, "pacman"),
        ];
        for (content, expected, tool) in cases {
            assert_eq!(family_from(content), expected, "{content:?}");
            assert!(advice_for(expected).contains(tool), "{content:?} should mention {tool}");
        }
    }

    #[test]
    fn an_unknown_distro_gets_generic_advice() {
        assert_eq!(family_from("ID=plan9\n"), Family::Other);
        assert!(advice_for(Family::Other).contains("extensions.gnome.org"));
    }

    #[test]
    fn advice_is_actionable() {
        // Whichever branch this host takes, the text must name a concrete next
        // step rather than just stating the problem.
        let advice = missing_tray_advice();
        assert!(advice.contains("No system tray found"));
        assert!(
            advice.contains("apt")
                || advice.contains("pacman")
                || advice.contains("extensions.gnome.org"),
            "advice must name a concrete install route, got: {advice}"
        );
    }
}
