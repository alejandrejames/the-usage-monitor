#!/usr/bin/env bash
# bump-version.sh — bump the app version and tag a release.
#
# Updates the workspace version in Cargo.toml, moves the CHANGELOG
# [Unreleased] section under the new version, commits, and creates an
# annotated git tag (vX.Y.Z).
#
# Cargo.toml is the only place the version lives: crates/app inherits it via
# `version.workspace = true`, and Tauri reads the crate version because
# `version` is omitted from tauri.conf.json. One edit reaches the binary, the
# bundle and every installer filename.
#
# Usage:
#   ./Scripts/bump-version.sh patch      # 1.1.0 -> 1.1.1
#   ./Scripts/bump-version.sh minor      # 1.1.1 -> 1.2.0
#   ./Scripts/bump-version.sh major      # 1.2.0 -> 2.0.0
#   ./Scripts/bump-version.sh 1.4.2      # set an explicit version
#
# Add --no-tag to skip the git commit and tag (only edit files).

set -euo pipefail
cd "$(dirname "$0")/.."

MANIFEST="Cargo.toml"
CHANGELOG="CHANGELOG.md"

[ -f "$MANIFEST" ] || { echo "✗ $MANIFEST not found"; exit 1; }

# ── Portable in-place sed ───────────────────────────────────────────────────
#
# BSD sed (macOS) requires an argument to -i; GNU sed (Linux, Git Bash) rejects
# one. The previous version hardcoded the BSD form and so could not run on the
# Linux and Windows machines this project now targets.
if sed --version >/dev/null 2>&1; then
  sed_i() { sed -i "$@"; }          # GNU
else
  sed_i() { sed -i '' "$@"; }       # BSD
fi

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
#
# Read from the [workspace.package] table. Anchored to a line starting with
# `version` so dependency versions elsewhere in the file cannot match.
CURRENT=$(grep -E '^version[[:space:]]*=' "$MANIFEST" | head -1 | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/')
[ -n "$CURRENT" ] || { echo "✗ Could not read the workspace version from $MANIFEST"; exit 1; }

IFS='.' read -r MAJ MIN PAT <<< "$CURRENT"

# ── Compute next version ────────────────────────────────────────────────────
case "$KIND" in
  major) NEW="$((MAJ + 1)).0.0" ;;
  minor) NEW="${MAJ}.$((MIN + 1)).0" ;;
  patch) NEW="${MAJ}.${MIN}.$((PAT + 1))" ;;
  [0-9]*.[0-9]*.[0-9]*) NEW="$KIND" ;;
  *) echo "✗ Unknown bump kind: $KIND"; exit 1 ;;
esac

echo "▶ $CURRENT  →  $NEW"

# ── Edit Cargo.toml ─────────────────────────────────────────────────────────
sed_i -E "s/^(version[[:space:]]*=[[:space:]]*\")[0-9]+\.[0-9]+\.[0-9]+(\")/\1${NEW}\2/" "$MANIFEST"

# Keep Cargo.lock in step so the tree is clean after the bump.
if command -v cargo >/dev/null 2>&1; then
  CARGO_BIN=$(rustup which cargo 2>/dev/null || command -v cargo)
  "$CARGO_BIN" metadata --format-version 1 >/dev/null 2>&1 \
    && echo "  ✓ Cargo.lock refreshed"
fi

# ── Update CHANGELOG: rename [Unreleased] to the new version + date ──────────
if [ -f "$CHANGELOG" ]; then
  TODAY=$(date +%Y-%m-%d)
  sed_i -E "s/^## \[Unreleased\]\$/## [Unreleased]\n\n## [${NEW}] - ${TODAY}/" "$CHANGELOG"
  echo "  ✓ CHANGELOG.md updated (Unreleased notes moved under [$NEW])"
fi

if [ "$NO_TAG" -eq 1 ]; then
  echo "✅ Files updated. (--no-tag: skipped git commit + tag)"
  exit 0
fi

# ── Commit + tag ────────────────────────────────────────────────────────────
git add "$MANIFEST" "$CHANGELOG" Cargo.lock 2>/dev/null || true
git commit -m "Release v${NEW}" >/dev/null
git tag -a "v${NEW}" -m "v${NEW}"
echo "✅ Committed and tagged v${NEW}."
echo "   Push with:  git push && git push origin v${NEW}"
