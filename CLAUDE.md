# CLAUDE.md — ClaudeUsage

This file is the primary context for Claude Code. Read it fully before generating or modifying any file in this project.

---

## What this project is

A native macOS menubar app that shows Claude.ai subscription usage (session % and weekly %) in the menu bar, with a Liquid Glass popover on click. Includes an iOS companion app and a WidgetKit home-screen widget. Personal use only — no App Store distribution.

**Target users:** The developer running this on their own Mac. There is no multi-user requirement.

---

## Tech stack

| Layer | Choice | Notes |
|---|---|---|
| Language | Swift 5.10 | No Objective-C |
| UI | SwiftUI + AppKit bridge where needed | 70 % SwiftUI, 30 % AppKit |
| State | `@Observable` macro (Swift 5.9+) | No Combine, no ObservableObject |
| Styling | Liquid Glass — `.glassEffect()` | Requires macOS 26 / iOS 26. Use `.ultraThinMaterial` fallback for older OS |
| Minimum targets | macOS 26, iOS 26 | Xcode 26 required |
| Networking | `URLSession` with async/await | No third-party HTTP libraries |
| Keychain | `Security` framework directly | No wrappers |
| Widget | WidgetKit | Reads App Group UserDefaults — no direct networking |
| Build | `Scripts/build.sh` | xcodebuild → create-dmg |
| Signing | Personal Team (free Apple ID) | No paid Developer ID, no notarization |

---

## Project structure

```
ClaudeUsage/
├── CLAUDE.md                        ← you are here
├── ClaudeUsage.xcodeproj            ← Xcode project (3 targets)
│
├── Shared/                          ← compiled into macOS + iOS targets
│   ├── Theme.swift                  ClaudeGlass modifier, UsageBar, color helpers
│   ├── AuthManager.swift            WKWebView cookie login + Keychain
│   └── UsageStore.swift             @Observable store, UsagePoller, App Group writes
│
├── ClaudeUsage/                     ← macOS menubar target
│   ├── ClaudeUsageApp.swift         @main, MenuBarExtra, MenuBarLabel, LoginPromptView
│   ├── PopoverView.swift            Liquid Glass usage panel shown on click
│   ├── LoginView.swift              WKWebView sheet, auto-dismisses on cookie capture
│   └── SettingsView.swift           Account, polling interval, notification alerts
│
├── ClaudeUsageIOS/                  ← iOS companion target
│   └── ClaudeUsageIOSApp.swift      @main, IOSMainView (usage cards), IOSLoginView
│
├── ClaudeUsageWidget/               ← WidgetKit extension target
│   └── ClaudeUsageWidget.swift      TimelineProvider, SmallWidgetView, MediumWidgetView
│
├── Scripts/
│   └── build.sh                     Full build pipeline
├── ExportOptions.plist              Personal Team export config
├── README.md
└── docs/
    ├── architecture.md
    ├── auth.md
    ├── liquid-glass.md
    ├── data-flow.md
    ├── widget.md
    └── build-dmg.md
```

---

## Xcode project requirements

Three targets must exist in `ClaudeUsage.xcodeproj`:

| Target | Type | Bundle ID | Deployment |
|---|---|---|---|
| `ClaudeUsage` | macOS App | `com.you.claudeusage` | macOS 26 |
| `ClaudeUsageIOS` | iOS App | `com.you.claudeusage.ios` | iOS 26 |
| `ClaudeUsageWidget` | Widget Extension | `com.you.claudeusage.widget` | iOS 26 |

**Capabilities required on all targets:**
- App Groups → `group.com.you.claudeusage`
- Keychain Sharing → `com.you.claudeusage`

**`Shared/` target membership:**

| File | macOS | iOS | Widget |
|---|---|---|---|
| `Theme.swift` | ✓ | ✓ | ✓ |
| `AuthManager.swift` | ✓ | ✓ | — |
| `UsageStore.swift` | ✓ | ✓ | — |

The Widget target does **not** get `AuthManager` or `UsageStore`. It only reads the `UserDefaults` suite `group.com.you.claudeusage` written by `UsageStore`.

---

## Authentication — WebView session cookie

See `docs/auth.md` for full detail. Summary:

- Login method: **WebView only** — no manual token paste, no OAuth CLI flow.
- The app presents `WKWebView` loading `https://claude.ai/login`.
- `AuthManager` conforms to `WKHTTPCookieStoreObserver` and fires when the `sessionKey` cookie appears.
- `sessionKey` is saved to Keychain via `KeychainService` with `kSecAttrAccessibleWhenUnlocked`.
- On subsequent launches, `AuthManager.init()` checks Keychain — if present, `isAuthenticated = true` immediately and the login view is skipped.
- `AuthManager.logout()` deletes the Keychain entry and sets `isAuthenticated = false`.

**Do not implement** any fallback to OAuth token paste or `claude setup-token`. WebView login is the only method.

---

## Data flow

See `docs/data-flow.md` for the full diagram. Summary:

1. `UsagePoller` fires every 60 seconds (configurable via `@AppStorage("refreshInterval")`).
2. It reads `sessionKey` from `AuthManager.sessionKey` (which reads Keychain).
3. It calls `GET https://api.claude.ai/api/oauth/usage` with:
   - `Cookie: sessionKey=<value>`
   - `User-Agent: claude-code/1.0.0` — **required** to avoid 429 rate-limiting
   - `Accept: application/json`
4. Response is decoded into `UsageResponse` (`limits.sessionUsagePercent`, `limits.weeklyUsagePercent`, `limits.sessionResetAt`, `limits.weeklyResetAt`).
5. `UsageStore` is updated and also writes to `UserDefaults(suiteName: "group.com.you.claudeusage")` for the widget.
6. If polling fails, `UsageStore.isStale = true` and the menu bar icon fades to 50% opacity.

---

## Liquid Glass styling

See `docs/liquid-glass.md` for full API reference. Rules:

- **Use `.claudeGlass()`** (defined in `Theme.swift`) on any floating surface: popovers, cards, sheets.
- **Use `.buttonStyle(.glass)`** on all buttons.
- **Never** apply `.glassEffect()` to scrollable content, lists, or full-screen backgrounds.
- **Never** nest `.glassEffect()` inside another `.glassEffect()`. Use `GlassEffectContainer` when grouping.
- Colours come from `Double.usageColor`: green `< 70%`, amber `< 90%`, red `≥ 90%`.
- Named colors (`UsageGreen`, `UsageAmber`, `UsageRed`) must be added to the Asset Catalog in each target.

```swift
// Correct pattern
VStack { content }
    .claudeGlass()

// Correct for buttons
Button("Refresh") { }
    .buttonStyle(.glass)

// Wrong — never do this
List { rows }
    .claudeGlass()
```

---

## Key design decisions

- **No third-party dependencies.** Everything is native Apple frameworks.
- **`@Observable` not `ObservableObject`.** Use the macro. Do not use `@Published`.
- **Polling not push.** There is no WebSocket or background daemon. `Timer` is sufficient.
- **Widget reads App Group, not the network.** The widget never calls the API itself.
- **macOS `LoginView` uses `NSViewRepresentable`.** The iOS version uses `UIViewRepresentable`. Both are in `LoginView.swift` with `#if os(macOS)` guards.
- **`MenuBarExtra` with `.menuBarExtraStyle(.window)`.** This gives a floating panel, not a dropdown menu.
- **`@AppStorage` for preferences** (`refreshInterval`, `alertThreshold80`, `alertThreshold95`). No custom persistence layer.

---

## Coding conventions

- File header comment: one line describing the file's role, no date or author.
- `// MARK: - SectionName` to separate logical sections within a file.
- `private` on all internal methods and properties.
- `final` on all classes that are not meant to be subclassed.
- `weak self` in all closures that capture `self` from a class.
- `DispatchQueue.main.async` for all UI updates from background threads.
- No force-unwrap (`!`) except for compile-time-guaranteed values like hardcoded URLs.
- Trailing commas in multi-line collections.

---

## What Claude Code should NOT do

- Do not add SPM packages or CocoaPods.
- Do not create view models — use `@Observable` stores directly.
- Do not use `Combine` or `@Published`.
- Do not add a manual token input field or any non-WebView auth path.
- Do not create unit test targets unless explicitly asked.
- Do not modify `ExportOptions.plist` — it is pre-configured.
- Do not change bundle IDs — they must match the App Group entitlement.
- Do not use `UserDefaults` for the session key — Keychain only.
- Do not add `.glassEffect()` to content-layer views (lists, scroll views).

---

## Generating the Xcode project

Claude Code cannot create `.xcodeproj` files. The workflow is:

1. Create the project manually in Xcode (macOS App template, SwiftUI interface).
2. Add the `ClaudeUsageIOS` and `ClaudeUsageWidget` targets manually.
3. Enable App Groups and Keychain Sharing on all targets.
4. Drop the generated `.swift` files into the correct target folders.
5. In Xcode's file inspector, tick the correct target memberships for each `Shared/` file.

Ask Claude Code to generate `.swift` files and `Scripts/build.sh`. Do not ask it to generate `.xcodeproj`.

---

## Building and running

```bash
# Build DMG
chmod +x Scripts/build.sh
./Scripts/build.sh

# First install on this Mac
open build/*.dmg
# Drag to Applications, then right-click → Open on first launch

# Or strip quarantine
xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app
```

---

## Reference docs in this repo

| File | Contents |
|---|---|
| `docs/architecture.md` | Component map and target structure |
| `docs/auth.md` | WebView login flow in detail |
| `docs/liquid-glass.md` | `.glassEffect()` API quick reference |
| `docs/data-flow.md` | Poll cycle, App Group writes, widget reads |
| `docs/widget.md` | WidgetKit timeline, sizes, App Group key names |
| `docs/build-dmg.md` | Step-by-step build and install guide |
