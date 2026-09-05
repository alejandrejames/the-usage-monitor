#!/usr/bin/env bash
# build-linux.sh — build the Linux bundles inside the container.
#
# Produces .deb, .rpm and .AppImage under target-linux/release/bundle/.
#
# Run from the repo root:
#   docker build -f packaging/Dockerfile.linux -t claudeusage-linux .
#   ./packaging/build-linux.sh
#
# AppImage is the recommended artifact for Bazzite and other rpm-ostree
# systems: layering a package there requires `rpm-ostree install` plus a
# reboot, which is exactly the friction those distros exist to avoid.

set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE="claudeusage-linux"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "▶ Building the $IMAGE image first…"
  docker build -f packaging/Dockerfile.linux -t "$IMAGE" .
fi

# Named volumes keep the registry and build cache warm between runs; the
# target dir is deliberately separate from the host's ./target so a Linux
# build never clobbers the macOS one.
docker run --rm \
  -v "$PWD":/src \
  -v claudeusage-cargo:/root/.cargo/registry \
  -v claudeusage-target:/src/target-linux \
  "$IMAGE" \
  bash -c '
    set -euo pipefail
    export CARGO_TARGET_DIR=/src/target-linux
    cargo install tauri-cli --version "^2" --locked 2>/dev/null || true
    cargo tauri build --config crates/app/tauri.conf.json
  '

echo
echo "✅ Bundles are under target-linux/release/bundle/"
echo "   Bazzite/Silverblue: prefer the .AppImage over the .rpm."
