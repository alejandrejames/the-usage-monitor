# Changelog

All notable changes to ClaudeUsage are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Versioning lives in `project.yml` (`MARKETING_VERSION` = the semver below,
`CURRENT_PROJECT_VERSION` = the build number). Use `Scripts/bump-version.sh`
to bump both and tag a release.

## [Unreleased]

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
