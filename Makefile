# ClaudeUsage — task runner.
#
#   make            list every target
#   make check      what CI would run: fmt, clippy, tests
#   make run        launch the app
#
# One cross-platform Rust/Tauri app. The original Swift/Xcode app it replaced
# was retired once parity landed; see docs/cross-platform.md for the port.

SHELL := /bin/bash
.DEFAULT_GOAL := help

# ── Toolchain ───────────────────────────────────────────────────────────────
#
# Homebrew's rust (1.85) shadows rustup on PATH on some machines, and Tauri v2
# needs 1.88+. Resolving cargo through rustup sidesteps that without asking
# anyone to reorder their PATH. Falls back to plain `cargo` if rustup is absent.
CARGO := $(shell rustup which cargo 2>/dev/null || command -v cargo)

# Prepending the toolchain's bin to PATH is not redundant with $(CARGO):
# subcommands like `cargo clippy` and `cargo fmt` are separate binaries that
# cargo finds on PATH, so without this they would run Homebrew's older
# cargo-clippy and fail on the same rustc floor.
TOOLCHAIN_BIN := $(dir $(CARGO))
export PATH := $(TOOLCHAIN_BIN):$(PATH)

# Where the Linux container writes, kept apart from the host's ./target so a
# container build never clobbers a macOS one.
LINUX_IMAGE  := claudeusage-linux
LINUX_TARGET := /src/target-linux
DOCKER_RUN    = docker run --rm \
	-v "$(PWD)":/src \
	-v claudeusage-cargo:/root/.cargo/registry \
	-v claudeusage-target:$(LINUX_TARGET) \
	$(LINUX_IMAGE) bash -c

.PHONY: help
help: ## Show this help
	@echo "ClaudeUsage — available targets"
	@echo
	@grep -hE '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "Using cargo: $(CARGO)"

# ── Develop ─────────────────────────────────────────────────────────────────

.PHONY: run
run: ## Run the app (tray + popover)
	$(CARGO) run -p claudeusage

.PHONY: dev
dev: ## Run via the Tauri CLI, with devtools enabled
	cd crates/app && $(CARGO) tauri dev

.PHONY: poll
poll: ## One live usage + status poll, no GUI
	$(CARGO) run -q -p claudeusage --bin pollcheck

.PHONY: probe
probe: ## Show which credential source resolves, and the plan/expiry
	$(CARGO) run -q -p claudeusage-core --bin probe

# ── Verify ──────────────────────────────────────────────────────────────────

.PHONY: check
check: fmt-check lint test ## Everything CI would run

.PHONY: test
test: ## Run the workspace tests
	$(CARGO) test --workspace

.PHONY: lint
lint: ## Clippy, warnings are errors
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format the workspace
	$(CARGO) fmt --all

.PHONY: fmt-check
fmt-check: ## Fail if anything is unformatted
	$(CARGO) fmt --all --check

# ── Build ───────────────────────────────────────────────────────────────────

.PHONY: build
build: ## Build the app in release mode (no bundle)
	$(CARGO) build --release -p claudeusage

.PHONY: bundle
bundle: ## Build the macOS .app and .dmg
	cd crates/app && $(CARGO) tauri build
	@echo
	@echo "▶ Bundles under target/release/bundle/"
	@echo "  Unsigned, so first launch needs:"
	@echo "    xattr -rd com.apple.quarantine /Applications/ClaudeUsage.app"

.PHONY: icons
icons: ## Regenerate the icon set from crates/app/icons/icon.png
	@# Tauri's bundler needs .icns/.ico plus specific PNG sizes; a lone
	@# 1024x1024 fails with "No matching IconType". Delete the mobile output
	@# it also emits, which this project has no targets for.
	cd crates/app && $(CARGO) tauri icon icons/icon.png
	rm -rf crates/app/icons/android crates/app/icons/ios \
		crates/app/icons/Square*.png crates/app/icons/StoreLogo.png

.PHONY: clean
clean: ## Remove Rust build artifacts
	$(CARGO) clean

# ── Cross-platform ──────────────────────────────────────────────────────────

.PHONY: linux-image
linux-image: ## Build (or rebuild) the Ubuntu 22.04 build container
	docker build -f packaging/Dockerfile.linux -t $(LINUX_IMAGE) .

.PHONY: linux-image-if-missing
linux-image-if-missing:
	@# Only build when absent. Making linux-check depend on linux-image
	@# unconditionally re-ran apt on every invocation, which is slow and fails
	@# outright when the Docker VM is low on disk. Use `make linux-image` to
	@# force a rebuild after changing the Dockerfile.
	@docker image inspect $(LINUX_IMAGE) >/dev/null 2>&1 \
		|| $(MAKE) linux-image

.PHONY: linux-check
linux-check: linux-image-if-missing ## Compile, lint and test for Linux in the container
	$(DOCKER_RUN) 'export CARGO_TARGET_DIR=$(LINUX_TARGET); \
		cargo clippy --workspace --all-targets -- -D warnings && \
		cargo test --workspace'

.PHONY: linux-bundle
linux-bundle: ## Build the .deb, .rpm and .AppImage
	./packaging/build-linux.sh

.PHONY: windows-check
windows-check: ## Type-check the Windows-only code paths
	@# The app crate cannot be cross-checked from macOS: ring, via
	@# ureq/rustls, needs a C compiler targeting Windows. The core crate and
	@# the cfg-gated Windows branches inside it do check, which catches
	@# wrong Win32 symbols and bad feature flags. See docs/cross-platform.md.
	rustup target add x86_64-pc-windows-msvc
	$(CARGO) check -p claudeusage-core --target x86_64-pc-windows-msvc

# ── Release ─────────────────────────────────────────────────────────────────

.PHONY: version
version: ## Show the app version
	@# One source of truth: crates/app inherits this via version.workspace,
	@# and Tauri reads the crate version because `version` is omitted from
	@# tauri.conf.json. Verified end to end — see docs/cross-platform.md.
	@grep -m1 '^version' Cargo.toml | cut -d'"' -f2

.PHONY: bump
bump: ## Bump the version and tag (KIND=patch|minor|major|X.Y.Z)
	@test -n "$(KIND)" || { echo "Usage: make bump KIND=patch"; exit 1; }
	./Scripts/bump-version.sh $(KIND)
