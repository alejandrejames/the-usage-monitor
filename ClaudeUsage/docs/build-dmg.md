# Build and DMG

## Prerequisites (one-time)

```bash
# Xcode 26 must be installed and Command Line Tools active
xcode-select --install

# Node.js and create-dmg
brew install node graphicsmagick imagemagick
npm install --global create-dmg

# Verify
create-dmg --version
```

---

## Xcode signing setup (one-time)

1. Open `ClaudeUsage.xcodeproj` in Xcode.
2. Select the `ClaudeUsage` target → Signing & Capabilities.
3. Team: select your Apple ID (shows as "Your Name (Personal Team)").
4. Bundle Identifier: `com.you.claudeusage` — replace `you` with your own identifier.
5. Repeat for `ClaudeUsageIOS` (`com.you.claudeusage.ios`) and `ClaudeUsageWidget` (`com.you.claudeusage.widget`).
6. Update `ExportOptions.plist` — replace `YOUR_TEAM_ID` with the 10-character team ID shown in Xcode (e.g. `AB12CD34EF`).

---

## Building the DMG

```bash
cd ClaudeUsage          # project root
chmod +x Scripts/build.sh
./Scripts/build.sh
```

The script runs three steps:

```
1. xcodebuild archive      → build/ClaudeUsage.xcarchive
2. xcodebuild exportArchive → build/export/ClaudeUsage.app
3. create-dmg               → build/ClaudeUsage 1.0.0.dmg
```

Output: `build/ClaudeUsage 1.0.0.dmg`

---

## Installing on your Mac

### Option A — Finder (recommended for first install)

```bash
open build/*.dmg
```

Drag `ClaudeUsage.app` to `Applications`. On first launch, macOS Gatekeeper will block the app with:

> "ClaudeUsage.app" can't be opened because Apple cannot check it for malicious software.

Right-click (or Control-click) `ClaudeUsage.app` in Finder → **Open** → **Open**. This bypasses Gatekeeper for this app permanently.

### Option B — Remove quarantine via Terminal

```bash
# After dragging to Applications:
xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app
```

The app will open normally on all subsequent launches.

---

## Why no notarization

Notarization requires a paid Apple Developer Program account ($99/year). Since this app is for personal use on your own Mac, it is not needed. The right-click → Open bypass works indefinitely.

If you later want to share the app with others, you will need to:
1. Join the Apple Developer Program.
2. Sign with a Developer ID certificate.
3. Notarize with `xcrun notarytool`.
4. Staple the notarization ticket to the DMG.

---

## `ExportOptions.plist` reference

```xml
<dict>
  <key>method</key>
  <string>development</string>       <!-- Personal Team; no paid cert needed -->

  <key>signingStyle</key>
  <string>automatic</string>         <!-- Xcode picks the right cert -->

  <key>teamID</key>
  <string>YOUR_TEAM_ID</string>      <!-- Replace: 10-char ID from Xcode -->

  <key>destination</key>
  <string>export</string>

  <key>stripSwiftSymbols</key>
  <true/>
</dict>
```

---

## Rebuild after code changes

```bash
./Scripts/build.sh
```

The script cleans `build/` on every run. After it completes, copy the new `.app` over the existing one in Applications:

```bash
cp -R build/export/ClaudeUsage.app /Applications/ClaudeUsage.app
```

No need to remove quarantine again after replacing the app this way.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| `xcodebuild: error: No account for team` | Sign in to Xcode with your Apple ID via Xcode → Settings → Accounts |
| `create-dmg: command not found` | Run `npm install --global create-dmg` |
| DMG created but app won't open | Run `xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app` |
| Keychain items not accessible after rebuild | Use Personal Team (stable identity), not ad-hoc signing |
| `Error: No signing certificate "iOS Distribution" found` | You are using the wrong export method. Confirm `ExportOptions.plist` has `method = development` |
