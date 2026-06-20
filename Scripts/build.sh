#!/bin/bash
# build.sh — ClaudeUsage full build pipeline
# Produces: build/ClaudeUsage <version>.dmg
#
# Prerequisites:
#   brew install xcodegen            # regenerates the .xcodeproj from project.yml
# The DMG is built with macOS's built-in hdiutil — no Node/create-dmg needed.
#
# Signing: prefers a proper signed archive (needs a valid Apple ID session in
# Xcode for the Personal Team); automatically falls back to an ad-hoc local
# build that runs on this Mac when that login is unavailable.
#
# Usage: chmod +x build.sh && ./build.sh

set -euo pipefail

# Always operate from the repo root (this script lives in Scripts/).
cd "$(dirname "$0")/.."

SCHEME="ClaudeUsage"
PROJECT="ClaudeUsage.xcodeproj"
BUILD_DIR="$(pwd)/build"
ARCHIVE_PATH="$BUILD_DIR/ClaudeUsage.xcarchive"
EXPORT_PATH="$BUILD_DIR/export"
EXPORT_PLIST="$(pwd)/ExportOptions.plist"
DERIVED_PATH="$BUILD_DIR/DerivedData"

# ── Helpers ────────────────────────────────────────────────────────────────

# Build the app without contacting Apple, then ad-hoc sign the app and its
# embedded widget WITH their entitlements (so the App Group still works).
# Echoes the path to the built .app on success; exits on failure.
build_adhoc() {
  xcodebuild build \
    -project "$PROJECT" \
    -scheme  "$SCHEME" \
    -configuration Release \
    -derivedDataPath "$DERIVED_PATH" \
    CODE_SIGN_IDENTITY="-" \
    CODE_SIGN_STYLE=Manual \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGNING_ALLOWED=NO \
    PROVISIONING_PROFILE_SPECIFIER="" \
    -quiet >&2

  local app
  app=$(find "$DERIVED_PATH/Build/Products" -maxdepth 3 -name "$SCHEME.app" | head -n 1)
  if [ -z "$app" ]; then
    echo "  ✗ Ad-hoc build produced no .app" >&2
    exit 1
  fi

  # Re-sign embedded widget first, then the app (inside-out), ad-hoc WITH
  # entitlements. CODE_SIGNING_ALLOWED=NO strips entitlements, so without this
  # the App Group is lost and the widget can't read the synced values.
  local widget="$app/Contents/PlugIns/$SCHEME""Widget.appex"
  local app_ent="$(pwd)/$SCHEME/$SCHEME.entitlements"
  local wid_ent="$(pwd)/$SCHEME""Widget/$SCHEME""Widget.entitlements"
  if [ -d "$widget" ] && [ -f "$wid_ent" ]; then
    codesign --force --sign - --entitlements "$wid_ent" --timestamp=none "$widget" >&2
  fi
  [ -f "$app_ent" ] && codesign --force --sign - --entitlements "$app_ent" --timestamp=none "$app" >&2

  echo "$app"
}

# Wrap a .app into a compressed DMG with an /Applications drop link, using the
# built-in hdiutil (no Node/create-dmg dependency). Echoes the DMG path.
make_dmg() {
  local app="$1"
  local version stage dmg
  version=$(defaults read "$app/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null || echo "0.0.0")
  stage="$BUILD_DIR/dmg-stage"
  dmg="$BUILD_DIR/$SCHEME $version.dmg"

  rm -rf "$stage" "$dmg"
  mkdir -p "$stage"
  cp -R "$app" "$stage/"
  ln -s /Applications "$stage/Applications"

  hdiutil create -volname "$SCHEME $version" \
    -srcfolder "$stage" -ov -format UDZO "$dmg" >&2
  rm -rf "$stage"
  echo "$dmg"
}

echo ""
echo "╔══════════════════════════════════╗"
echo "║  ClaudeUsage · build pipeline   ║"
echo "╚══════════════════════════════════╝"
echo ""

# ── 0. (Re)generate the Xcode project from project.yml ─────────────────────
# The .xcodeproj is generated, not committed. Regenerate it if XcodeGen is
# installed (brew install xcodegen) or if it doesn't exist yet.
if command -v xcodegen >/dev/null 2>&1; then
  echo "▶ Generating $PROJECT from project.yml…"
  xcodegen generate
elif [ ! -d "$PROJECT" ]; then
  echo "  ✗ $PROJECT not found and XcodeGen is not installed."
  echo "    Run: brew install xcodegen && xcodegen generate"
  exit 1
fi

# ── 1. Clean previous build ────────────────────────────────────────────────
echo "▶ Cleaning build directory…"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# ── 2. Build the .app ──────────────────────────────────────────────────────
# Preferred path: a properly-signed archive + export (needs a valid Apple ID
# session in Xcode so a provisioning profile can be minted for the Personal
# Team). When that login is unavailable — common with a free Apple ID whose
# Xcode session has lapsed — we fall back to an ad-hoc local build that still
# runs on this Mac. The fallback re-signs the app and its embedded widget
# ad-hoc WITH their entitlements so App Groups keeps working.
echo "▶ Archiving ($SCHEME)…"
if xcodebuild archive \
     -project      "$PROJECT" \
     -scheme       "$SCHEME" \
     -configuration Release \
     -archivePath  "$ARCHIVE_PATH" \
     CODE_SIGN_STYLE=Automatic \
     -allowProvisioningUpdates \
     -quiet
then
  echo "  ✓ Archive at $ARCHIVE_PATH"

  echo "▶ Exporting .app…"
  xcodebuild -exportArchive \
    -archivePath       "$ARCHIVE_PATH" \
    -exportOptionsPlist "$EXPORT_PLIST" \
    -exportPath        "$EXPORT_PATH" \
    -quiet
  APP_PATH=$(find "$EXPORT_PATH" -name "*.app" | head -n 1)
  echo "  ✓ App exported: $APP_PATH"
else
  echo "  ⚠ Signed archive failed (likely no Apple ID session for provisioning)."
  echo "    Falling back to an ad-hoc local build (runs on this Mac only)."
  APP_PATH=$(build_adhoc)
  echo "  ✓ Ad-hoc app built: $APP_PATH"
fi

# ── 3. Wrap in DMG ─────────────────────────────────────────────────────────
# Built-in hdiutil — no Node/create-dmg dependency, always available.
echo "▶ Creating DMG…"
DMG_PATH=$(make_dmg "$APP_PATH")

if [ -z "$DMG_PATH" ] || [ ! -f "$DMG_PATH" ]; then
  echo "  ✗ DMG not created — check output above"
  exit 1
fi

echo "  ✓ DMG ready: $DMG_PATH"

# ── 5. Remove quarantine (personal machine only) ───────────────────────────
echo ""
echo "▶ To install on this Mac, run:"
echo "  open \"$DMG_PATH\""
echo "  Then drag ClaudeUsage.app to Applications."
echo "  On first launch: right-click → Open (bypasses Gatekeeper once)."
echo ""
echo "  Or remove quarantine immediately:"
echo "  xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app"
echo ""
echo "✅ Done."
