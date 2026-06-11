#!/bin/bash
# bump-version.sh — bump the app version and tag a release.
#
# Updates MARKETING_VERSION (semver) and CURRENT_PROJECT_VERSION (build number)
# in project.yml, moves the CHANGELOG [Unreleased] section to the new version,
# commits, and creates an annotated git tag (vX.Y.Z).
#
# Usage:
#   ./Scripts/bump-version.sh patch      # 1.0.0 -> 1.0.1
#   ./Scripts/bump-version.sh minor      # 1.0.1 -> 1.1.0
#   ./Scripts/bump-version.sh major      # 1.1.0 -> 2.0.0
#   ./Scripts/bump-version.sh 1.4.2      # set an explicit version
#
# Add --no-tag to skip git commit/tag (only edit files).

set -euo pipefail
cd "$(dirname "$0")/.."

PROJECT_YML="project.yml"
CHANGELOG="CHANGELOG.md"

[ -f "$PROJECT_YML" ] || { echo "✗ $PROJECT_YML not found"; exit 1; }

# ── Parse args ──────────────────────────────────────────────────────────────
NO_TAG=0
KIND=""
for arg in "$@"; do
  case "$arg" in
    --no-tag) NO_TAG=1 ;;
    *)        KIND="$arg" ;;
  esac
done
[ -n "$KIND" ] || { echo "Usage: $0 <major|minor|patch|X.Y.Z> [--no-tag]"; exit 1; }

# ── Current version ─────────────────────────────────────────────────────────
CURRENT=$(grep -E '^\s*MARKETING_VERSION:' "$PROJECT_YML" | head -1 | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/')
BUILD=$(grep -E '^\s*CURRENT_PROJECT_VERSION:' "$PROJECT_YML" | head -1 | sed -E 's/.*"([0-9]+)".*/\1/')
[ -n "$CURRENT" ] || { echo "✗ Could not read MARKETING_VERSION from $PROJECT_YML"; exit 1; }

IFS='.' read -r MAJ MIN PAT <<< "$CURRENT"

# ── Compute next version ────────────────────────────────────────────────────
case "$KIND" in
  major) NEW="$((MAJ + 1)).0.0" ;;
  minor) NEW="${MAJ}.$((MIN + 1)).0" ;;
  patch) NEW="${MAJ}.${MIN}.$((PAT + 1))" ;;
  [0-9]*.[0-9]*.[0-9]*) NEW="$KIND" ;;
  *) echo "✗ Unknown bump kind: $KIND"; exit 1 ;;
esac
NEW_BUILD=$((BUILD + 1))

echo "▶ $CURRENT (build $BUILD)  →  $NEW (build $NEW_BUILD)"

# ── Edit project.yml ────────────────────────────────────────────────────────
sed -i '' -E "s/(MARKETING_VERSION:[[:space:]]*\")[0-9.]+(\")/\1${NEW}\2/" "$PROJECT_YML"
sed -i '' -E "s/(CURRENT_PROJECT_VERSION:[[:space:]]*\")[0-9]+(\")/\1${NEW_BUILD}\2/" "$PROJECT_YML"

# ── Update CHANGELOG: rename [Unreleased] to the new version + date ──────────
if [ -f "$CHANGELOG" ]; then
  TODAY=$(date +%Y-%m-%d)
  # Insert a fresh empty [Unreleased] above the dated section.
  sed -i '' -E "s/^## \[Unreleased\]\$/## [Unreleased]\n\n## [${NEW}] - ${TODAY}/" "$CHANGELOG"
  echo "  ✓ CHANGELOG.md updated (move Unreleased notes under [$NEW])"
fi

# ── Regenerate the Xcode project if XcodeGen is available ────────────────────
if command -v xcodegen >/dev/null 2>&1; then
  xcodegen generate >/dev/null && echo "  ✓ Regenerated ClaudeUsage.xcodeproj"
fi

if [ "$NO_TAG" -eq 1 ]; then
  echo "✅ Files updated. (--no-tag: skipped git commit + tag)"
  exit 0
fi

# ── Commit + tag ────────────────────────────────────────────────────────────
git add "$PROJECT_YML" "$CHANGELOG" 2>/dev/null || true
git commit -m "Release v${NEW}" >/dev/null
git tag -a "v${NEW}" -m "v${NEW}"
echo "✅ Committed and tagged v${NEW}."
echo "   Push with:  git push && git push origin v${NEW}"
