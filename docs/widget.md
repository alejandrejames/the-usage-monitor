# Widget

## Overview

`ClaudeUsageWidget` is a WidgetKit extension that shows Claude usage on the
**macOS desktop / Notification Center** (macOS Sonoma+). It does **not** call the
Anthropic API and has **no keychain access** — it reads a display-safe snapshot
from the App Group `group.com.you.claudeusage` that the menu-bar app writes after
each poll.

```
UsagePoller (app)  ──poll──▶  UsageStore.update()
                                   │ writes display-safe values (no token)
                                   ▼
                       App Group: group.com.you.claudeusage
                                   │ WidgetCenter.reloadAllTimelines()
                                   ▼
                       ClaudeUsageWidget (separate process)
```

## Data contract

The shared payload is `WidgetUsage` (see `Shared/SharedUsage.swift`), stored as
JSON under the key `widgetUsage` in `UserDefaults(suiteName:)`:

| Field | Meaning |
|---|---|
| `sessionPercent` / `weeklyPercent` | 0–100 utilisation |
| `sessionResetAt` / `weeklyResetAt` | reset timestamps |
| `lastUpdated` | last successful poll |
| `plan` | subscription plan (e.g. "pro") |
| `isStale` | last poll failed / offline |

The OAuth token is **never** written here (CLAUDE.md security rule). Read/write go
through `SharedUsage.read()` / `SharedUsage.write(_:)`.

## Supported sizes

| Family | View | Content |
|---|---|---|
| `.systemSmall` | `SmallView` | Session % ring, colour-coded |
| `.systemMedium` | `MediumView` | Session + Weekly bars + reset countdowns |
| `.systemLarge` | `LargeView` | Both bars, plan, "updated Xm ago" |

All sizes reuse `Double.usageColor` and `UsageBar` from `Shared/Theme.swift`, so
colours match the app: green < 70 %, amber < 90 %, red ≥ 90 %.

## Refresh

The app drives prompt refreshes via `WidgetCenter.shared.reloadAllTimelines()`
on every write. As a fallback (e.g. app closed), the timeline also refreshes on a
~15-minute policy so countdowns don't drift. WidgetKit does not guarantee exact
timing and may batch refreshes.

When the last poll failed, `WidgetUsage.isStale` is true and the views dim
slightly and show a "disconnected" indicator.

## Target setup (handled by project.yml)

The `ClaudeUsageWidget` target is an `app-extension` with the WidgetKit extension
point. Its sources are `ClaudeUsageWidget/` plus only `Shared/Theme.swift` and
`Shared/SharedUsage.swift` — **not** `AuthManager`/`UsageStore` (no networking in
the widget). The widget has its own `Assets.xcassets` carrying the `UsageGreen/
Amber/Red` colours so `usageColor` resolves in the widget bundle.

## App Group capability (free Personal Team)

Both targets carry the entitlement:

```xml
<key>com.apple.security.application-groups</key>
<array>
    <string>group.com.you.claudeusage</string>
</array>
```

The free Personal Team supports **local** App Groups (unlike iCloud KVS, which is
why the iOS sync was dropped). On first build, open Xcode → Signing &
Capabilities for both `ClaudeUsage` and `ClaudeUsageWidget` and confirm the App
Group is checked; Xcode registers it on first build.

## Adding the widget

Right-click the desktop → **Edit Widgets**, find **Claude Usage**, and drop the
size you want. (The menu-bar app must have run at least once so a snapshot
exists; otherwise the widget shows an "Open ClaudeUsage" placeholder.)
