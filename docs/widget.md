# Widget

## Overview

`ClaudeUsageWidget` is a WidgetKit extension that displays usage data on the iPhone home screen. It does **not** call the Anthropic API directly — it reads from the App Group `UserDefaults` container written by `UsageStore` in the macOS or iOS app.

---

## Supported sizes

| Family | View | Content |
|---|---|---|
| `systemSmall` | `SmallWidgetView` | Session % large number + mini bar |
| `systemMedium` | `MediumWidgetView` | Session + Weekly side-by-side |

Lock screen and StandBy sizes are not implemented in v1.

---

## Timeline

`UsageProvider` conforms to `TimelineProvider`:

```swift
func getTimeline(in context: Context, completion: @escaping (Timeline<UsageEntry>) -> Void) {
    let entry    = readEntry()
    let nextDate = Calendar.current.date(byAdding: .minute, value: 15, to: Date())!
    completion(Timeline(entries: [entry], policy: .after(nextDate)))
}
```

WidgetKit refreshes every 15 minutes. This is the minimum practical interval — WidgetKit does not guarantee exact timing and may batch refreshes.

If the app has not polled recently (> 5 minutes), `UsageEntry.isStale = true` and widgets should visually indicate stale data (e.g. reduced opacity on the percentage label).

---

## Reading App Group data

```swift
private func readEntry() -> UsageEntry {
    let defaults = UserDefaults(suiteName: "group.com.you.claudeusage")
    let session  = defaults?.double(forKey: "sessionPercent") ?? 0
    let weekly   = defaults?.double(forKey: "weeklyPercent")  ?? 0
    let sReset   = defaults?.double(forKey: "sessionResetAt").map { Date(timeIntervalSince1970: $0) }
    let wReset   = defaults?.double(forKey: "weeklyResetAt").map  { Date(timeIntervalSince1970: $0) }
    let updated  = defaults?.double(forKey: "lastUpdated").map    { Date(timeIntervalSince1970: $0) }
    let isStale  = updated.map { Date().timeIntervalSince($0) > 300 } ?? true

    return UsageEntry(
        date: Date(),
        sessionPercent: session,
        weeklyPercent: weekly,
        sessionReset: sReset,
        weeklyReset: wReset,
        isStale: isStale
    )
}
```

App Group `UserDefaults` keys:

| Key | Type stored | Meaning |
|---|---|---|
| `sessionPercent` | `Double` | 0–100 |
| `weeklyPercent` | `Double` | 0–100 |
| `sessionResetAt` | `Double` (Unix timestamp) | When the 5-hour window resets |
| `weeklyResetAt` | `Double` (Unix timestamp) | When the weekly quota resets |
| `lastUpdated` | `Double` (Unix timestamp) | When `UsageStore` last wrote |

---

## Liquid Glass in widgets

iOS 26 widgets automatically receive a Liquid Glass background when you use `.containerBackground(.clear, for: .widget)`. No additional modifier is needed.

```swift
var body: some View {
    VStack { content }
        .padding(14)
        .containerBackground(.clear, for: .widget)  // ← glass applied by system
}
```

Do **not** call `.claudeGlass()` or `.glassEffect()` inside a widget view — widget extensions do not have access to the same rendering context and the modifier will be ignored or crash.

---

## Xcode setup for the widget target

1. In Xcode, add a new target: File → New → Target → Widget Extension.
2. Name it `ClaudeUsageWidget`. Bundle ID: `com.you.claudeusage.widget`.
3. Uncheck "Include Configuration Intent" (this is a static widget).
4. Add the App Groups capability: `group.com.you.claudeusage`.
5. Add `Theme.swift` to the widget target (for `Double.usageColor`).
6. Do **not** add `AuthManager.swift` or `UsageStore.swift` to the widget target.

---

## Placeholder and snapshot

`getSnapshot` is called by WidgetKit when showing a preview in the widget gallery. Use realistic-looking data:

```swift
func placeholder(in context: Context) -> UsageEntry {
    UsageEntry(date: Date(), sessionPercent: 72, weeklyPercent: 41,
               sessionReset: nil, weeklyReset: nil, isStale: false)
}
```

`getSnapshot` should return the same as `readEntry()` (real data if available, placeholder values if not).

---

## App Group entitlement format

The entitlement key in the `.entitlements` file for the widget extension must be:

```xml
<key>com.apple.security.application-groups</key>
<array>
    <string>group.com.you.claudeusage</string>
</array>
```

Xcode adds this automatically when you enable App Groups in the Signing & Capabilities tab.
