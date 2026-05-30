# Architecture

## Target structure

```
ClaudeUsage.xcodeproj
│
├── ClaudeUsage          macOS 26   com.you.claudeusage
├── ClaudeUsageIOS       iOS 26     com.you.claudeusage.ios
└── ClaudeUsageWidget    iOS 26     com.you.claudeusage.widget
```

All three targets share the App Group `group.com.you.claudeusage` and the Keychain access group `com.you.claudeusage`.

---

## Component map

```
┌─ macOS process ─────────────────────────────────────────────┐
│                                                             │
│  ClaudeUsageApp (@main)                                     │
│    └─ MenuBarExtra (.window style)                          │
│         ├─ MenuBarLabel          icon + percentage          │
│         └─ PopoverView           Liquid Glass panel         │
│              ├─ UsageBar (×2)    session + weekly           │
│              └─ SettingsView     on sheet                   │
│                                                             │
│  AuthManager (@Observable)                                  │
│    ├─ WKWebView → claude.ai/login                           │
│    ├─ WKHTTPCookieStoreObserver  detects sessionKey         │
│    └─ KeychainService            save / load / delete       │
│                                                             │
│  UsageStore (@Observable)                                   │
│    ├─ sessionPercent, weeklyPercent                         │
│    ├─ sessionResetAt, weeklyResetAt                         │
│    └─ writeToAppGroup()  → UserDefaults suite               │
│                                                             │
│  UsagePoller                                                │
│    ├─ Timer (60 s, configurable)                            │
│    └─ GET api.claude.ai/api/oauth/usage                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘

         ↓ App Group UserDefaults (non-secret display data)

┌─ iOS process ───────────────────────────────────────────────┐
│                                                             │
│  ClaudeUsageIOSApp (@main)                                  │
│    ├─ IOSLoginView               if not authenticated       │
│    └─ IOSMainView                usage cards (Liquid Glass) │
│         └─ AuthManager + UsagePoller  same as macOS         │
│                                                             │
└─────────────────────────────────────────────────────────────┘

         ↑ reads same App Group UserDefaults

┌─ Widget extension ──────────────────────────────────────────┐
│                                                             │
│  ClaudeUsageWidgetBundle (@main)                            │
│    └─ ClaudeUsageWidget                                     │
│         └─ UsageProvider (TimelineProvider)                 │
│              ├─ SmallWidgetView   session % only            │
│              └─ MediumWidgetView  session + weekly          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## File responsibilities

### Shared/

| File | Responsibility |
|---|---|
| `Theme.swift` | `ClaudeGlass` `ViewModifier`, `UsageBar` view, `Double.usageColor` extension |
| `AuthManager.swift` | `KeychainService` enum, `AuthManager` `@Observable` class, cookie observation |
| `UsageStore.swift` | `UsageResponse` Codable model, `UsageStore` `@Observable` class, `UsagePoller` class |

### ClaudeUsage/ (macOS)

| File | Responsibility |
|---|---|
| `ClaudeUsageApp.swift` | `@main`, `MenuBarExtra` scene, `MenuBarLabel`, `LoginPromptView` |
| `PopoverView.swift` | Main usage panel: header, two `UsageBar`s, Refresh + Settings buttons |
| `LoginView.swift` | `WebView` (`NSViewRepresentable`), `LoginView` with auto-dismiss |
| `SettingsView.swift` | Account card, polling interval picker, alert toggles, notification request |

### ClaudeUsageIOS/

| File | Responsibility |
|---|---|
| `ClaudeUsageIOSApp.swift` | `@main`, `IOSMainView` (usage cards with Liquid Glass), `IOSLoginView` |

### ClaudeUsageWidget/

| File | Responsibility |
|---|---|
| `ClaudeUsageWidget.swift` | `UsageEntry`, `UsageProvider`, `SmallWidgetView`, `MediumWidgetView`, `@main` bundle |

---

## State ownership

- `AuthManager` is owned by the `@main` App struct as `@State`. One instance per process.
- `UsageStore` is owned by the `@main` App struct as `@State`. One instance per process.
- `UsagePoller` is owned by the `@main` App struct. Started in `PopoverView.onAppear`, stopped in `onDisappear`.
- The Widget has no state objects — it reads `UserDefaults` directly in `UsageProvider.readEntry()`.

---

## Shared data keys (App Group UserDefaults)

Suite name: `group.com.you.claudeusage`

| Key | Type | Written by | Read by |
|---|---|---|---|
| `sessionPercent` | `Double` | `UsageStore` | Widget `UsageProvider` |
| `weeklyPercent` | `Double` | `UsageStore` | Widget `UsageProvider` |
| `sessionResetAt` | `Double` (Unix timestamp) | `UsageStore` | Widget `UsageProvider` |
| `weeklyResetAt` | `Double` (Unix timestamp) | `UsageStore` | Widget `UsageProvider` |
| `lastUpdated` | `Double` (Unix timestamp) | `UsageStore` | Widget (stale check) |

The Keychain holds the `sessionKey` cookie value only. It is never written to `UserDefaults`.
