# Changelog

All notable changes to ClaudeUsage are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Versioning lives in `project.yml` (`MARKETING_VERSION` = the semver below,
`CURRENT_PROJECT_VERSION` = the build number). Use `Scripts/bump-version.sh`
to bump both and tag a release.

## [Unreleased]

### Added
- `docs/cross-platform.md` — plan of record for porting the app to Tauri v2
  (macOS, Windows, Linux).
- `crates/core` (`claudeusage-core`) — platform-agnostic port of the usage
  header parsing, Statuspage status parsing, threshold colours and
  edge-triggered alerts, with 43 unit tests. No UI and no OS calls, so it
  builds on all three target platforms. The macOS Swift app is unaffected and
  remains the reference implementation.
- Credential sources in `claudeusage-core`: an ordered chain on macOS
  (credentials file, `/usr/bin/security`, native keychain) and the plaintext
  `.credentials.json` on Linux and Windows, honouring `CLAUDE_CONFIG_DIR`.
  Includes a `probe` binary for verifying the chain on any platform.
- `crates/app` and `ui/` — the Tauri v2 host and popover. Renders the tray icon
  with a bundled DejaVu Sans Bold (tabular digits, so the icon does not jitter),
  polls usage and service status on background threads, and fires the 80/95 %
  notifications. The popover is plain HTML/CSS/JS; reset times are formatted
  with `Intl.DateTimeFormat`. A `pollcheck` binary verifies a live poll outside
  the GUI. macOS only so far.
- Windows support in `crates/app`: the tray icon is rendered at the DPI-queried
  size (`GetSystemMetricsForDpi`), since Windows downscales an oversized icon
  poorly, and a 250 ms debounce stops a tray click from reopening the popover
  that the preceding focus-loss just closed. Written and cross-compile-checked
  but not yet run on Windows hardware.
- Linux support in `crates/app`: the tray icon is redrawn only when its colour
  bucket changes (every `set_icon` writes a PNG to `$XDG_RUNTIME_DIR`, so a
  per-poll redraw would mean ~1,440 writes a day), and a startup check reports
  when no StatusNotifierWatcher is present with install advice matched to the
  distro. `packaging/` adds a containerised Ubuntu 22.04 build producing
  `.deb`, `.rpm` and `.AppImage`; AppImage is recommended for Bazzite.

## [1.1.0] - 2026-06-11

### Added
- **macOS desktop widget** (Small / Medium / Large) showing session and weekly
  usage, with the same colour thresholds as the app. Reads a display-safe
  snapshot from an App Group; it never networks or touches the keychain.
- Option to **hide the menu-bar icon** (Settings → Menu bar) so the widget can
  be the only surface. Re-launching the app reopens Settings, so it's never
  unreachable.
- The Settings window now opens automatically on launch / re-launch.
- App icon (timer + spark) wired into the asset catalog.

### Fixed
- Closing the Settings window no longer quits the app — it keeps running in the
  menu bar / as the widget's data source. Quit is explicit (Settings → Quit).
- "Show menu bar icon" toggle now reliably hides/shows the item and persists the
  choice (driven by App-owned state rather than an `@AppStorage` binding that
  SwiftUI didn't re-evaluate).

### Security
- Only display-safe usage values cross the App Group boundary; the OAuth token
  is never written to the shared container.

## [1.0.0] - 2026-06-11

### Added
- Native macOS menu-bar app showing Claude session (5-hour) and weekly (7-day)
  usage, reusing Claude Code's OAuth token from the login keychain.
- Liquid Glass popover with usage bars and reset countdowns.
- Selectable menu-bar display mode: session only, weekly only, or both
  (stacked, rendered as an image so both rows are always visible).
- Per-metric usage colouring (green < 70%, amber < 90%, red ≥ 90%) on the
  menu-bar icon, percentages, and popover bars.
- Usage threshold notifications at 80% and 95%.
- Configurable polling interval (30s / 60s / 2m / 5m).
- Automatic refresh when network connectivity is restored.

### Changed
- In-memory token cache: routine polls (including post-reconnect) reuse the
  cached token instead of re-reading the keychain, avoiding repeated macOS
  keychain prompts. The keychain is only re-read on launch, expiry, or a 401.

### Security
- The OAuth token is never written to disk, `UserDefaults`, or the App Group —
  only display-safe percentages and reset dates are persisted.
- `.gitignore` excludes credential and certificate files, plus local
  `CLAUDE.md` / `.claude/` config.

[Unreleased]: https://github.com/alejandrejames/the-usage-monitor/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/alejandrejames/the-usage-monitor/releases/tag/v1.0.0
