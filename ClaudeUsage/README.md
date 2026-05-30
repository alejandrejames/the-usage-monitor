# ClaudeUsage

macOS menubar app that shows your Claude.ai subscription usage (session % and weekly %)
with Liquid Glass styling (macOS Tahoe / iOS 26).

---

## Project structure

```
ClaudeUsage/
├── Shared/
│   ├── Theme.swift          — Liquid Glass modifier + UsageBar + colour helpers
│   ├── AuthManager.swift    — WKWebView login + Keychain storage
│   └── UsageStore.swift     — @Observable store + UsagePoller + App Group writes
│
├── ClaudeUsage/             — macOS menubar target
│   ├── ClaudeUsageApp.swift — MenuBarExtra entry point + label
│   ├── PopoverView.swift    — Liquid Glass usage panel (on click)
│   ├── LoginView.swift      — WKWebView login sheet
│   └── SettingsView.swift   — Interval, alerts, sign out
│
├── ClaudeUsageIOS/          — iOS companion target
│   └── ClaudeUsageIOSApp.swift  — Usage cards + iOS login
│
├── ClaudeUsageWidget/       — WidgetKit extension target
│   └── ClaudeUsageWidget.swift  — TimelineProvider + small/medium views
│
├── Scripts/
│   └── build.sh             — archive → export → create-dmg pipeline
└── ExportOptions.plist      — Personal Team export config
```

---

## Xcode setup (one time)

1. Open Xcode → New Project → macOS → App
2. Add targets: `ClaudeUsageIOS` (iOS → App), `ClaudeUsageWidget` (iOS → Widget Extension)
3. Add `Shared/` files to all three targets
4. In every target → Signing & Capabilities:
   - Add **App Groups** → `group.com.you.claudeusage`
   - Add **Keychain Sharing** → `com.you.claudeusage`
5. Set deployment targets: **macOS 26**, **iOS 26**
6. Replace `YOUR_TEAM_ID` in `ExportOptions.plist`

---

## Building the DMG

```bash
# Install tools (one time)
brew install node graphicsmagick imagemagick
npm install --global create-dmg

# Build
chmod +x Scripts/build.sh
./Scripts/build.sh
```

Output: `build/ClaudeUsage 1.0.0.dmg`

**First-time install on your Mac:**

```bash
# Option A — right-click → Open in Finder (one-time Gatekeeper bypass)
open build/*.dmg

# Option B — remove quarantine flag immediately
xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app
```

---

## Liquid Glass surface map

| Surface | Modifier |
|---|---|
| Popover panel (macOS) | `.claudeGlass()` on the root `VStack` |
| Settings section cards | `.claudeGlass()` on each card |
| Login header overlay | `.claudeGlass()` on the header `HStack` |
| iOS usage cards | `.claudeGlass()` on each card |
| All buttons | `.buttonStyle(.glass)` |
| iOS widget background | `.containerBackground(.clear, for: .widget)` |

---

## Data flow

```
claude.ai WebView login
    └─ sessionKey cookie → Keychain
           └─ UsagePoller (60 s) → internal OAuth endpoint
                  └─ UsageStore (@Observable)
                         ├─ macOS: MenuBarLabel + PopoverView
                         └─ App Group UserDefaults
                                └─ WidgetKit TimelineProvider (15 min)
                                       └─ iOS widget views
```

---

## Sources

- WebView session cookie approach: github.com/linuxlewis/claude-usage
- OAuth endpoint info: github.com/rjwalters/claude-monitor
- Liquid Glass API: developer.apple.com (WWDC 2025 Session 219 + 323)
- create-dmg: github.com/sindresorhus/create-dmg
