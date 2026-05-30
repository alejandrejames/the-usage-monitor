#!/bin/bash
# build.sh — ClaudeUsage full build pipeline
# Produces: build/ClaudeUsage <version>.dmg
#
# Prerequisites:
#   brew install node graphicsmagick imagemagick
#   npm install --global create-dmg
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

# ── 2. Archive ─────────────────────────────────────────────────────────────
echo "▶ Archiving ($SCHEME)…"
xcodebuild archive \
  -project      "$PROJECT" \
  -scheme       "$SCHEME" \
  -configuration Release \
  -archivePath  "$ARCHIVE_PATH" \
  CODE_SIGN_STYLE=Automatic \
  -quiet

echo "  ✓ Archive at $ARCHIVE_PATH"

# ── 3. Export .app ─────────────────────────────────────────────────────────
echo "▶ Exporting .app…"
xcodebuild -exportArchive \
  -archivePath       "$ARCHIVE_PATH" \
  -exportOptionsPlist "$EXPORT_PLIST" \
  -exportPath        "$EXPORT_PATH" \
  -quiet

APP_PATH=$(find "$EXPORT_PATH" -name "*.app" | head -n 1)
echo "  ✓ App exported: $APP_PATH"

# ── 4. Wrap in DMG ─────────────────────────────────────────────────────────
echo "▶ Creating DMG…"
npx create-dmg \
  "$APP_PATH" \
  "$BUILD_DIR/" \
  --overwrite 2>/dev/null || true  # exits non-zero if no signing cert; DMG still created

DMG_PATH=$(find "$BUILD_DIR" -maxdepth 1 -name "*.dmg" | head -n 1)

if [ -z "$DMG_PATH" ]; then
  echo "  ✗ DMG not found — check create-dmg output above"
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
