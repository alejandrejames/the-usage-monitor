# CLAUDE.md — ClaudeUsage

This file is the primary context for Claude Code. Read it fully before generating or modifying any file in this project.

> **Status (2026-05-30): macOS-only, single target.** The iOS app and WidgetKit
> widget were **removed** — the only Mac→iPhone sync path (iCloud KVS) requires
> a paid Apple Developer account, which the free Personal Team can't use. Ignore
> the iOS/widget references that remain in the tables/docs below; only the
> `ClaudeUsage` macOS target exists. Also note: the auth/endpoint design changed
> entirely (see the Authentication and Data flow sections) — reads Claude Code's
> OAuth token and uses the `/v1/messages` header technique, **not** WebView
> sessionKey. And this approach is **against Anthropic's Consumer ToS**; it's a
> personal, never-distributed tool (see README's ToS notice).

---

## What this project is

A native macOS menu-bar app that shows your Claude subscription usage (5-hour
session % and 7-day weekly %) in the menu bar, with a Liquid Glass popover on
click. Personal use only — no App Store distribution, macOS-only.

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
| Auth | Reuse Claude Code's OAuth token via `Security` framework | Reads keychain item `Claude Code-credentials`; macOS unsandboxed |
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
│   ├── AuthManager.swift            Reads Claude Code's OAuth token from keychain (macOS); App Group viewer (iOS)
│   └── UsageStore.swift             @Observable store, UsagePoller (Bearer), App Group read/write
│
├── ClaudeUsage/                     ← macOS menubar target
│   ├── ClaudeUsageApp.swift         @main, MenuBarExtra, MenuBarLabel, NoCredentialView
│   ├── PopoverView.swift            Liquid Glass usage panel shown on click
│   └── SettingsView.swift           Account status, polling interval, notification alerts
│
├── ClaudeUsageIOS/                  ← iOS companion target (viewer only)
│   └── ClaudeUsageIOSApp.swift      @main, IOSMainView (usage cards), IOSWaitingView
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

**Capabilities:**
- App Groups → `group.com.you.claudeusage` (all three targets)
- **macOS app: App Sandbox OFF** — required to read Claude Code's login-keychain
  token. No Keychain Sharing group is needed (unsandboxed access).

**`Shared/` target membership:**

| File | macOS | iOS | Widget |
|---|---|---|---|
| `Theme.swift` | ✓ | ✓ | ✓ |
| `AuthManager.swift` | ✓ | ✓ | — |
| `UsageStore.swift` | ✓ | ✓ | — |

The Widget target does **not** get `AuthManager` or `UsageStore`. It only reads the `UserDefaults` suite `group.com.you.claudeusage` written by `UsageStore`.

---

## Authentication — reuse Claude Code's OAuth token

> **Reality check (2026-05-30):** The original WebView/`sessionKey` design did
> not work. `api.claude.ai` does not resolve, and `claude.ai/api/oauth/usage`
> sits behind a Cloudflare bot challenge that a plain `URLSession` cannot pass.
> The app now reuses Claude Code's existing OAuth token. There is no in-app
> login UI.

- **macOS:** `AuthManager` reads Claude Code's credential from the **login
  keychain** (generic-password, service `Claude Code-credentials`), decodes the
  JSON, and exposes `claudeAiOauth.accessToken` (an `sk-ant-oat01-…` Bearer
  token) plus `expiresAt` and `subscriptionType`.
- `isAuthenticated` is true when a non-expired token is present. There is no
  sign-in and no `logout()` — the user manages auth via the `claude` CLI. The
  popover shows a "Claude Code not detected" prompt with a **Re-check** button
  when no token is found.
- **The macOS app is NOT sandboxed.** A sandboxed app cannot read a login-
  keychain item it didn't create. App Sandbox is intentionally off (personal,
  non-App-Store tool). App Groups still works unsandboxed.
- **iOS has no token.** It cannot read the Mac's keychain, so it never fetches.
  It is a **viewer**: `AuthManager` (iOS branch) and `UsageStore.loadFromAppGroup()`
  read the values the macOS app synced into the App Group.

**Do not** reintroduce WebView/`sessionKey` login, OAuth token paste, or a
custom OAuth flow. The token comes from Claude Code's keychain item.

---

## Data flow

See `docs/data-flow.md` for the full diagram. Summary:

1. `UsagePoller` fires every 60 seconds (configurable via `@AppStorage("refreshInterval")`). Polling starts at launch, not when the popover opens.
2. It reads the Bearer token from `AuthManager.accessToken` (which reads Claude Code's keychain item).
3. It calls `GET https://api.anthropic.com/api/oauth/usage` with:
   - `Authorization: Bearer <accessToken>`
   - `Accept: application/json`
4. Response is decoded into `UsageResponse`: `five_hour.utilization`/`resets_at` (the 5-hour "session" window) and `seven_day.utilization`/`resets_at` (the weekly window). `resets_at` is ISO-8601 **with fractional seconds + offset**, so the decoder uses a custom `ISO8601DateFormatter` with `.withFractionalSeconds` (plain `.iso8601` fails).
5. `UsageStore` is updated and also writes display-safe values to `UserDefaults(suiteName: "group.com.you.claudeusage")` for the widget and iOS viewer.
6. On failure (network, decode) or HTTP 401 (expired/revoked token) `UsageStore.isStale = true`, the menu bar icon fades to 50% opacity, and `AuthManager.refreshAvailability()` re-checks the token.

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
- **Widget and iOS read the App Group, not the network.** Only the macOS app fetches; everything else reads the synced values.
- **No in-app login.** Auth is Claude Code's keychain token (see Authentication). No `LoginView`/WebView.
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
- Do not reintroduce WebView/`sessionKey` login or a manual token input field. Auth = Claude Code's keychain token.
- Do not re-enable the macOS App Sandbox — it blocks reading Claude Code's keychain item.
- Do not create unit test targets unless explicitly asked.
- Do not modify `ExportOptions.plist` — it is pre-configured.
- Do not change bundle IDs — they must match the App Group entitlement.
- Do not write the OAuth token to `UserDefaults` or the App Group — only display-safe percentages/dates are synced.
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
