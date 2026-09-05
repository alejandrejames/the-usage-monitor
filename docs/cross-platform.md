# The cross-platform port — a record

How ClaudeUsage went from a macOS-only Swift menu-bar app to a single codebase
running on **macOS, Windows, and Linux** (Ubuntu, Debian, Arch, Bazzite).

> **This is the record of the port, not a description of the system.**
> For how ClaudeUsage works today, read [architecture.md](architecture.md).
> This document is kept because the measurements behind several non-obvious
> decisions live here — the keychain timings, the tray-scaling behaviour, the
> Linux `set_icon` cost — and re-deriving them would be expensive.

Status: **complete.** All six phases done, plus the UI work that followed.
macOS runs and is in daily use. Linux compiles, lints and tests in a
container. Windows is cross-compile-checked only. **Neither Windows nor Linux
has been run on real hardware.**

---

## Why

ClaudeUsage is ~1,690 lines of Swift across 9 files. The port is worth doing
because the codebase is already cleanly split: roughly **60 % of the logic is
platform-agnostic** (HTTP polling, header parsing, Statuspage JSON, threshold
alerts) and only the menu-bar host is genuinely bound to AppKit.

Two things make the port *better* than a like-for-like rewrite:

- **Credential access gets simpler off macOS.** Only macOS uses a keychain.
  Linux and Windows store the same JSON at `~/.claude/.credentials.json` and
  `%USERPROFILE%\.claude\.credentials.json`. The keychain ACL friction described
  in `docs/auth.md` disappears on two of three platforms.
- **Dropping the WidgetKit widget deletes the App Group path hack** in
  `Shared/SharedUsage.swift`, which exists only because the free Personal Team
  does not truly provision App Groups.

### Locked decisions

| Decision | Choice |
|---|---|
| Framework | **Tauri v2** — Rust core + native WebView UI |
| WidgetKit widget | **Dropped entirely**, along with the App Group hack |
| Packaging | **Full installers, unsigned** (`.dmg` / `.msi` / `.deb` / AppImage) |
| macOS credentials | **Fallback chain**, shelling out to `/usr/bin/security` first |
| Windows tray | **Two lines anyway** — accept illegibility at 100 % DPI |

**Outcome:** one Rust/web codebase, three platforms, with the Xcode project
retired only after macOS reaches verified parity.

---

## Verified findings

These were confirmed against the live system and upstream sources, not assumed.
They are the reason several phases are ordered the way they are.

### 1. The macOS keychain ACL is hostile to unsigned binaries

Dumping the live keychain item (`security dump-keychain -a`) shows:

```
entry 0: applications (4):
  0: /Applications/ClaudeUsage.app (status -2147415734)
       requirement: cdhash H"766459b46bfed02277ca1781f222222b26320b1b"
  1: /private/tmp/probe (status -67068)          ← a prior unsigned probe: DENIED
  2: .../DerivedData/.../ClaudeUsage.app
       requirement: identifier "com.you.claudeusage" and anchor apple generic
                    and certificate leaf[subject.CN] = "Apple Development: ..."
  3: /usr/bin/security (OK)
entry 3: authorizations (1): partition_id
         description: apple-tool:                 ← ONLY apple-tool
```

Three consequences:

- The `/Applications` grant is pinned to a **raw cdhash**, which changes on every
  rebuild. "Always Allow" already dies on each release rebuild today. The Xcode
  Debug entry survives only because it uses a rebuild-stable *designated
  requirement*.
- The **`partition_id` list contains only `apple-tool:`**. This is a gate
  separate from the applications ACL — `/usr/bin/security` passes it, a
  third-party app does not, regardless of what the applications list says.
- Claude Code **rewrites the credential on every token refresh**, resetting both
  the ACL and the partition list and wiping any grant the user gave.

`~/.claude/.credentials.json` is **absent** on this machine, so the keychain is
currently the only macOS source — but Claude Code does write that file when a
keychain write is rejected, so the chain must handle it.

This is why the macOS credential path is a **chain, not a single call**. The
in-memory token cache in `Shared/AuthManager.swift` is load-bearing and masks how
often this would otherwise prompt; it must survive the port.

#### Spike A result — measured, not assumed

An ad-hoc-signed Rust probe (`security-framework` 3.7.0) was built and run
against the live item. Both paths return the same 3,306-byte credential:

| Run | Native `SecItemCopyMatching` | CLI `/usr/bin/security` |
|---|---|---|
| 1 (first ever) | **9.90 s** — ACL prompt shown | **31.8 ms** — no prompt |
| 2 (after rebuild, new cdhash) | **11.35 s** — ACL prompt shown **again** | **34.2 ms** — no prompt |
| 3–5 (CLI only) | — | no prompt, every time |

The ~10 s native timings are the prompt waiting on a human click; the CLI path is
sub-40 ms throughout. After clicking "Always Allow" on run 1 the probe was added
to the applications list as `(OK)` and the partition list grew from
`apple-tool:` to `apple-tool:, teamid:F7B9FG5D66, cdhash:175bf201…`.

**The grant is pinned to that cdhash.** A one-line source change moved the
binary to cdhash `d5bb3d7a…` and **the prompt returned on run 2**. This is
conclusive: for an unsigned/ad-hoc binary the ACL grant does not survive a
rebuild, so the native path can never be the primary source. The CLI-first chain
is validated and is the design of record.

The probe left an ACL entry behind, as expected.

### 2. Tray text is not portable

| Platform | `set_title` | `set_tooltip` |
|---|---|---|
| macOS | supported | supported |
| Linux | SNI only, requires an icon present | **unsupported** |
| Windows | **unsupported** | supported |

### 3. `set_icon` on Linux writes a PNG to disk on every call

The GTK/AppIndicator backend takes an `icon_path`, not a buffer, so each
`set_icon` writes `$XDG_RUNTIME_DIR/tray-icon/tray-icon-{id}-{n}.png`. At a 60 s
poll that is **~1,440 writes/day**. Linux must therefore use a **static icon +
`set_title`**, re-rendering the icon only when the threshold colour bucket
changes (green → amber → red).

### 4. GNOME has no native tray; Bazzite ships GNOME

Requires the AppIndicator / KStatusNotifierItem extension — the same requirement
every tray app on GNOME has (Steam, Discord, Nextcloud). **Do not build a
bespoke fallback window.** Detect whether `org.kde.StatusNotifierWatcher` is on
the session bus and show a one-time install hint instead (~30 lines versus a
large, fragile surface).

### 5. Spike B result — the icon is scaled, not clipped

Rendered two stacked rows with `ab_glyph` at several sizes and inspected the
PNGs:

| Buffer | Rows | Verdict |
|---|---|---|
| 44 × 44 px (macOS @2×) | `43%` / `71%` | clean and legible |
| 22 × 18 px (macOS @1×) | `43%` / `71%` | readable, tight |
| 32 px (Windows @200 %) | `43` / `71` | clean |
| 16 px (Windows @100 %) | `43` / `71` | ~6 px per row — marginal, as expected |

**The pixel-vs-point question is settled, and the answer is benign.** The tao
backend normalises the icon to a fixed height in *points* and derives the width
from the source aspect ratio:

```rust
let (width, height) = self.icon.inner.get_size();
let icon_height: f64 = 18.0;
let icon_width: f64 = (width as f64) / (height as f64 / icon_height);
```

So a 44 px-tall buffer is **scaled down to 18 pt, not clipped** — the risk the
plan flagged does not exist, and Retina sharpness comes for free by supplying a
2× buffer. The renderer should therefore target an **aspect ratio**, not an
absolute pixel height.

Two follow-ups this surfaced:

- **The tray height is 18 pt, not the 22 pt** the menu bar allows, because the
  backend hardcodes it. Rows must be sized against 18.
- **A redistributable font still has to be chosen.** The spike used Monaco,
  which is monospaced (so digits are inherently tabular) but is an Apple system
  font that cannot be bundled. `ab_glyph` also cannot parse `.ttc` collections,
  so the bundled file must be a plain `.ttf`.

### 6. Wayland cannot position windows client-side

There is no protocol for "place this window at these coordinates."
`tauri-plugin-positioner`'s `TrayCenter` **cannot work on Wayland**. This is a
design constraint, not a bug to fix — the Linux popover is a normal centred
window. `TrayCenter` is also known-broken on macOS multi-monitor
([plugins-workspace#724](https://github.com/tauri-apps/plugins-workspace/issues/724))
and only works after the first tray click.

---

## Target architecture

```
crates/
├── core/                    pure Rust, no UI, unit-testable
│   ├── credentials.rs       trait CredentialSource + per-OS impls + token cache
│   ├── usage.rs             POST /v1/messages, parse ratelimit headers
│   ├── status.rs            status.claude.com Statuspage poller
│   ├── model.rs             UsageSnapshot, ServiceHealth, thresholds
│   └── alerts.rs            edge-triggered 80/95 % crossing logic
├── app/                     Tauri host
│   ├── tray/
│   │   ├── render.rs        shared RGBA text renderer (ab_glyph + tiny-skia)
│   │   ├── macos.rs         two-line image via set_icon_with_as_template
│   │   ├── windows.rs       two-line image, DPI-queried size, + set_tooltip
│   │   └── linux.rs         static icon + set_title, SNI detection
│   ├── commands.rs          #[tauri::command] bridge to the WebView
│   └── main.rs
ui/                          popover + settings (web)
packaging/                   per-OS bundler config
```

### Credential abstraction

```rust
pub trait CredentialSource {
    fn load(&self) -> Result<Credentials>;
}
```

- **macOS** — ordered chain, logging which source won:
  1. `~/.claude/.credentials.json` (absent today, but written when a keychain
     write is rejected)
  2. `/usr/bin/security find-generic-password -s "Claude Code-credentials" -w` —
     **the only path that passes the `apple-tool:` partition gate without
     prompting**; ~15 ms subprocess, once per token refresh, not per poll
  3. Native `security-framework` `ItemSearchOptions` (service-only, mirroring
     the current Swift query)
- **Linux** — `$CLAUDE_CONFIG_DIR/.credentials.json` else `~/.claude/.credentials.json`
- **Windows** — `%USERPROFILE%\.claude\.credentials.json`

Port the in-memory cache and `invalidateToken()`-on-401 from `AuthManager.swift`,
adding a **floor so a 401 storm cannot become a prompt storm**.

Native keychain access uses `ItemSearchOptions`, not `find_generic_password`
(which requires an account the Swift code deliberately omits):

```rust
ItemSearchOptions::new()
    .class(ItemClass::generic_password())
    .service("Claude Code-credentials")
    .load_data(true)
    .limit(Limit::Max(1))
    .search()?
```

### Tray strategy

| Platform | Approach |
|---|---|
| **macOS** | Two-line rendered image; `set_icon_with_as_template` to avoid per-poll flicker. **Non-template + colour** — template mode forces monochrome and would kill the green/amber/red scheme |
| **Windows** | Two lines at the DPI-queried size (`GetSystemMetrics(SM_CXSMICON)`: 16/20/24/32). Cramped at 100 %, legible at 150 %+ — accepted. `set_tooltip` carries full detail |
| **Linux** | Static icon + `set_title("43% · 71%")`. Re-render icon only on colour-bucket change. Reset times go in the tray **menu** (no tooltip support) |

**Rendering:** `ab_glyph` + `tiny-skia`, with a **bundled font**
(`include_bytes!`) using **tabular digits** so `43%` and `100%` do not jitter the
icon width each poll. Skip `resvg` / `cosmic-text` — no shaping or bidi is needed
for two lines of digits.

**SF Symbols (`timer`, `calendar`, `wifi.slash`) do not exist off-Apple.** Drop
glyphs from the tray entirely — position and colour disambiguate (top = session,
bottom = weekly). Keep icons in the popover as inline SVG. For the disconnected
state, desaturate to grey rather than swapping in a glyph unreadable at 16 px.

### Deliberate simplifications

- **Reset-time formatting moves to the web UI.** Rust has no locale-aware
  `DateFormatter`; `chrono` cannot do it and `icu` is heavy. Pass epoch millis to
  the WebView and format with `Intl.DateTimeFormat`, which has full locale data
  for free. This deletes Rust code and preserves `SharedUsage.swift`'s behaviour.
- **Drop `NWPathMonitor`.** Replace with exponential backoff (~10 s floor). This
  approximates the current re-poll-on-reconnect at a fraction of the complexity.
- **Isolate the polling technique.** The app sends a real 1-token request every
  60 s purely to read rate-limit headers (~1,440 billed requests/day). Keep this
  behind one function in `usage.rs` with a comment, so it is a one-file swap if a
  dedicated usage endpoint ever appears.

---

## Versioning migration

The project already has working versioning that the port breaks. Today:

| Piece | Role |
|---|---|
| `project.yml` (`MARKETING_VERSION`, `CURRENT_PROJECT_VERSION`) | Source of truth |
| `CHANGELOG.md` | Keep a Changelog, semver |
| `Scripts/bump-version.sh` | Bumps both fields, moves `[Unreleased]`, regenerates the `.xcodeproj`, commits, tags |
| Git tags | `v1.0.0`, `v1.1.0` — annotated |

Current version: **1.1.0** (build 2).

Three problems:

- `project.yml` is XcodeGen-only and is on the Phase 6 deletion list — but it
  **currently holds the version number**. Tauri reads version from
  `tauri.conf.json` / `Cargo.toml` instead, so the source of truth must move.
- `bump-version.sh` uses BSD `sed -i ''`, which **fails on GNU sed** — it will
  not run on Linux or in Git Bash on Windows.
- The `xcodegen generate` step becomes dead once the Xcode project is retired.

**Migration** (done in Phase 6 — see that phase for what actually worked):

- Move the source of truth to the workspace `Cargo.toml` `[workspace.package]
  version`, with `tauri.conf.json` set to `"version": "../Cargo.toml"` so Tauri
  inherits it rather than duplicating it.
- Rewrite `bump-version.sh` to be portable: detect GNU vs BSD sed (or switch to a
  small Rust/`cargo` step), drop the `xcodegen` call, and update `Cargo.toml`
  instead of `project.yml`.
- Keep the CHANGELOG format and annotated-tag convention **unchanged** — they are
  already well-formed and carry over as-is.
- Continue from **1.1.0**; the port is not a product reset. First cross-platform
  release is **2.0.0** (the platform set and install story both change).

Note the MSI `ProductVersion` and every installer filename derive from this
version, so the three files must not drift.

---

## Roadmap

Ordered by **risk, not convenience**. The four things that can invalidate the
plan are spiked before any production code is written.

### Phase 0 — Spikes (1–2 days, throwaway code)

| Spike | Question | Success criteria |
|---|---|---|
| ~~**A. Keychain**~~ **DONE** | Does the fallback chain avoid the prompt from an unsigned binary? | ✅ **Confirmed.** CLI path: no prompt, <40 ms, 5/5 runs. Native path: prompt on first run *and* again after rebuild (cdhash-pinned grant). See Spike A result above |
| ~~**B. Tray render**~~ **DONE (macOS half)** | Two-line RGBA legibility, macOS Retina + Windows DPI | ✅ Rendered with `ab_glyph`; see the Spike B result below. Windows DPI still needs real hardware to confirm on-screen |
| **C. Linux** | Tray + popover on Bazzite/GNOME Wayland | Tray appears with the AppIndicator extension; confirm the popover cannot be tray-anchored |

### Phase 1 — Core crate ✅ **DONE**

Ported `UsageSnapshot` header parsing, `StatusStore` (component ids
`rwppv331jlwc` / `yyzkbfz2thpt` with name fallback), threshold colours
(`#1D9E75` / `#BA7517` / `#E24B4A` at < 70 / < 90 / ≥ 90), and edge-triggered
alerts. **43 tests, no UI**, `clippy -D warnings` clean.

Landed as `claudeusage-core` with modules `usage` / `status` / `alerts` /
`model`, plus `backoff_secs()` in the crate root replacing `NWPathMonitor`.

Two deliberate departures from the Swift original:

- **Reset times are epoch millis, not formatted strings.** Formatting moved to
  the WebView (`Intl.DateTimeFormat`) as planned, so `SharedUsage.swift`'s
  `resetString` has no Rust counterpart.
- **`overall_health()` derives precedence from a severity ordering** rather than
  the hardcoded if-chain in `StatusStore.swift`, so adding a `ServiceHealth`
  case cannot silently break the ranking. `Unknown` deliberately sorts *above*
  `Operational` but *below* every real problem — an unmatched component stays
  visible without masking a live outage.

The Xcode targets glob only `Shared/` and `ClaudeUsage/`, so the Swift build is
untouched and remains the reference implementation until Phase 6.

### Phase 2 — Credentials ✅ **DONE**

`CredentialSource` trait + backends + cache, plus the probe binary:

```
cargo run -p claudeusage-core --bin probe
```

Two things found by inspecting the live keychain item that the Swift version
does not account for:

- **The blob carries more than this app's token.** It also stores OAuth tokens
  for every MCP server the user has authorised. Nothing in the module logs the
  blob, a token, or any substring of one — the probe prints a character count.
- **Newer fields exist** (`rateLimitTier`, `refreshTokenExpiresAt`) that the
  decoder must ignore rather than reject.

The cached provider adds a **10-second refetch floor** on top of the ported
`AuthManager` cache, so a burst of 401s cannot become a burst of keychain
prompts.

**The probe stops at the first successful source.** Probing further reaches the
native keychain API, which blocks on the ACL dialog and hangs indefinitely in a
non-interactive shell — pass `--all` to force the full sweep. This is a probe
concern only; in the GUI a user is present to click.

### Phase 3 — macOS parity ✅ **DONE**

`crates/app` (Tauri host) and `ui/` (popover) landed; the app builds, runs,
installs its tray and polls live. Activation policy is `Accessory` and
focus-loss auto-hide is wired via `WindowEvent::Focused(false)`.

Verified with `cargo run -p claudeusage --bin pollcheck`, which drives the whole
chain outside the GUI:

```
USAGE OK
  session: 59%
  weekly:  31%
STATUS: ok
  claude.ai      Operational
  Claude Code    Operational
```

**The tray icon could not be screenshotted** — the session lacks Screen
Recording permission — so it was verified by rendering the identical code path
to PNG and inspecting that instead.

Two follow-ups this phase created, both for Phase 6:

- **The version is now in three places**: `Cargo.toml`, `project.yml`, and
  `tauri.conf.json`. Tauri's `"version": "../../Cargo.toml"` inheritance reads a
  *package* manifest and rejects a workspace root, so it is hardcoded for now.
  The versioning migration must reconcile all three.
- **`crates/app/icons/` holds generated placeholders**, not designed artwork.

#### Toolchain

Tauri's dependency tree needs **rustc 1.88+**; Homebrew's rust (1.85) shadows
rustup on `PATH` here, so `rust-toolchain.toml` pins the project to rustup's
stable. `.nvmrc` pins Node 22. There is no bundler or `package.json` — the UI is
plain HTML/CSS/JS, so Node is needed only by the Tauri CLI itself.

### Phase 4 — Windows ⚠️ **CODE DONE, UNVERIFIED ON HARDWARE**

Two Windows-specific problems fixed:

**1. DPI-correct icon sizing.** Phase 3 rendered a fixed 54 px buffer on every
platform. macOS scales that down cleanly, but Microsoft's guidance is explicit
that on Windows an icon which is too large "is subject to being downscaled (also
poorly) by the OS". `tray::sizing` now queries
`GetSystemMetricsForDpi(SM_CYSMICON, GetDpiForSystem())` per render, so moving
between monitors with different scaling re-renders at the right size. It falls
back to 16 px if the metric read fails.

Rendered with the shipping font at each Windows tray size:

| DPI | Icon | Per row | Verdict |
|---|---|---|---|
| 100 % | 16 px | ~8 px | cramped; **colour still reads** even when digits blur |
| 125 % | 20 px | ~9 px | tight but usable |
| 150 % | 24 px | ~11 px | legible |
| 200 % | 32 px | ~15 px | clean |

This is the tradeoff already accepted, and the colour channel carrying the
signal at 16 px makes it milder than feared.

**2. Tray-click debounce.** Clicking the tray while the popover is open delivers
focus-loss *first* and the click second, so the click would reopen the window
the focus-loss just closed and the popover would appear never to close. A 250 ms
guard suppresses that. Applied on **all** platforms, not just Windows — the
event ordering is not guaranteed anywhere.

#### What "cross-compile-checked" means, and does not

`cargo check --target x86_64-pc-windows-msvc` passes for `claudeusage-core` and
for the `sizing` module, and both were confirmed to be *genuinely* compiled by
deliberately breaking the Windows-only branches and watching the check fail
while the host build stayed green. That verifies the `cfg` branches, the
`windows-sys` feature flags, and the Win32 symbol names.

**It does not verify anything visual or behavioural.** The full app crate cannot
be cross-checked from macOS at all: `ring`, pulled in transitively by
`ureq`→`rustls`, needs a C compiler targeting Windows. Switching rustls to a
pure-Rust provider would enable it, but changing the production TLS stack to
suit the dev machine is the wrong trade.

**Still to confirm on real hardware:** that the tray icon appears and is legible
at each DPI, that the debounce feels right, that notifications fire (they need
a Start Menu shortcut + AppUserModelID, so they no-op in dev builds and work
only from the MSI), and that the credential file is found at
`%USERPROFILE%\.claude\.credentials.json`.

### Phase 5 — Linux ✅ **CODE DONE, COMPILES AND TESTS IN A CONTAINER**

Unlike Windows, Linux can be verified fairly deeply from macOS: `packaging/`
carries an Ubuntu 22.04 build image (the oldest release shipping
`webkit2gtk-4.1`), and **the whole app crate compiles there** — `tao`,
`libappindicator`, `tray-icon` and all. `ring` blocks the equivalent Windows
check; nothing blocks this one.

**A real bug fixed.** Phase 3's Linux branch set `set_title` but never set an
icon at all — and `set_title` only displays *if an icon is present*, so the tray
would have shown nothing. The icon is now set, but only when its colour bucket
changes (`tray::linux::IconBuckets`), which preserves the whole point of the
Linux path: every `set_icon` writes a PNG into `$XDG_RUNTIME_DIR`, so redrawing
per poll would mean ~1,440 writes a day. Bucket changes happen a handful of
times a day.

**Missing-tray detection.** `detect_tray_host()` asks the session bus whether
`org.kde.StatusNotifierWatcher` has an owner, via `gdbus` (ships with GLib, so
no new dependency). When absent it prints install advice keyed to the distro
family — and deliberately **does not** suggest `apt` on an rpm-ostree system,
where layering needs a reboot. Registration can race with shell startup, so a
single negative at launch is treated as advice, never as a hard gate.

**Packaging.** `.deb`, `.rpm` and `.AppImage` targets with their dependency
lists (`libwebkit2gtk-4.1-0`, `libayatana-appindicator3-1` and the rpm
equivalents). `packaging/build-linux.sh` drives the containerised build.
**AppImage is the recommended artifact for Bazzite** — layering a package on an
immutable OS is exactly the friction those distros exist to avoid.

#### What the container does and does not prove

Verified: the full app crate compiles for Linux, 88 tests pass (26 app + 62
core), clippy is clean, the distro detection maps Bazzite / Silverblue /
Kinoite / Debian / Ubuntu / Arch to the right advice, and `detect_tray_host()`
degrades to `Unknown` rather than panicking with no session bus.

Not verified — needs a real desktop: that the tray icon actually appears and
that `set_title` renders beside it, that the popover behaves on Wayland (where
it **cannot** be tray-anchored, so it is a centred window by design), that the
bundles install and run, and that notifications reach the desktop.

### Phase 6 — Retire Xcode ✅ **DONE**

> **The gate was waived.** This phase was defined as "only after macOS parity is
> verified", and that verification has not happened — the tray has never been
> seen on screen. The user chose to proceed anyway; git history retains the
> Swift sources, so the decision is reversible.

**Versioning migration.** `Cargo.toml`'s `[workspace.package] version` is now
the only place the version lives. The mechanism is the opposite of what the plan
assumed: Tauri does **not** accept a path to a `Cargo.toml` (only a semver
string or a `package.json` path), but **omitting `version` entirely** makes it
fall back to the crate's own version — which `crates/app` already inherits via
`version.workspace = true`.

Verified end to end: setting the workspace version to 1.1.99 produced
`ClaudeUsage_1.1.99_aarch64.dmg`.

`bump-version.sh` now edits `Cargo.toml`, drops the dead `xcodegen` step, and
**detects GNU vs BSD sed** — it previously hardcoded the BSD form and so could
not run on the Linux and Windows machines this project now targets. Both paths
were tested, including a run in the Linux container.

**Removed:** `ClaudeUsage.xcodeproj`, `project.yml`, `ExportOptions.plist`,
`Scripts/build.sh`, `ClaudeUsage/`, `Shared/`, `ClaudeUsageWidget/`, and
`docs/{architecture,data-flow,widget,liquid-glass,build-dmg}.md`.

**Kept, deliberately:**

- **`docs/auth.md`**, rewritten for the new implementation. The history of why
  the WebView/`sessionKey` approach failed is worth not relearning.
- **The original app icon.** It was about to be lost with the asset catalog —
  recovered from git and converted RGB→RGBA (Tauri rejects RGB). `make icons`
  regenerates the set; a lone 1024×1024 fails with `No matching IconType`.

`CLAUDE.md` and the README were rewritten: both described a Swift/Xcode project
that no longer exists.

---

## Cross-cutting additions

Table stakes for a tray app, absent from the current Swift feature set:

- **`tauri-plugin-single-instance`** — a tray app must not run twice.
- **`tauri-plugin-autostart`** — launch at login.
- **`tauri-plugin-notification`** for the 80/95 % alerts. Windows toasts
  **require a Start Menu shortcut + AppUserModelID**; they silently no-op in dev
  builds but work from the MSI.

---

## Files

**Reference (read, port, do not modify until Phase 6):**

| File | What to carry over |
|---|---|
| `Shared/AuthManager.swift` | Service-only query semantics, cache/invalidation policy |
| `Shared/UsageStore.swift` | Header names `anthropic-ratelimit-unified-{5h,7d}-{utilization,reset}`, alert edge-triggering |
| `ClaudeUsage/ClaudeUsageApp.swift` (lines ~209–280) | The two-line `NSImage` renderer — reference for the cross-platform renderer |
| `Shared/SharedUsage.swift` | Reset-time rules, to reimplement via `Intl.DateTimeFormat` |
| `Shared/Theme.swift` | Threshold boundaries and exact hex colours |
| `Shared/StatusStore.swift` (lines 64–68) | Statuspage component ids and status mapping |

**Created:** this document, then `crates/`, `ui/`, `packaging/`

**Deleted at Phase 6:** `ClaudeUsageWidget/`, `ClaudeUsage.xcodeproj`,
`project.yml`, `ExportOptions.plist`, `Scripts/build.sh`, `docs/widget.md`,
`docs/liquid-glass.md`, `docs/build-dmg.md`

---

## Verification

**Per phase:**

- ~~**Phase 0A**~~ — **done.** CLI path produced zero prompts across 5 runs
  including a post-rebuild run; native path prompted on every new cdhash.
- ~~**Phase 1**~~ — **done.** `cargo test -p claudeusage-core` → 43 passed,
  covering header parsing (200 *and* 429), threshold boundaries at exactly
  70/90, half-away-from-zero rounding matching Swift's `.rounded()`,
  case-insensitive header lookup, NaN/inf rejection, edge-triggered alerts
  firing once per crossing, and the Statuspage name-fallback.
- ~~**Phase 2**~~ — **done.** `cargo run -p claudeusage-core --bin probe`
  resolves via the security CLI on macOS, and via the file source (the Linux and
  Windows path) when `CLAUDE_CONFIG_DIR` points at one. Both verified.
- ~~**Phase 3**~~ — **done on macOS.** App builds and runs, tray installs,
  `pollcheck` resolves credentials, usage headers and status against the live
  API. Still to confirm by hand: popover open/auto-hide behaviour and that the
  80/95 % notifications fire, both of which need an interactive session.
- **Phases 4–5** — on each OS: tray renders and updates; popover opens on click
  and auto-hides on focus loss; 80/95 % notifications fire once; the app survives
  a token refresh without prompting; the disconnected state renders when offline.

**End-to-end (all three platforms):**

1. Launch with a valid Claude Code session; the tray shows both percentages
   within one poll.
2. Run `claude` to force a token refresh; confirm the app recovers without a
   prompt (macOS) and without a stale reading.
3. Kill the network; confirm the disconnected state, then restore and confirm the
   backoff re-poll.
4. Install from the built installer (not `cargo run`) and confirm notifications
   fire — the only way to catch the Windows AppUserModelID issue.
5. Bazzite: confirm the AppImage runs and the tray appears with the AppIndicator
   extension enabled.

---

## Risks

| Risk | Mitigation |
|---|---|
| ~~macOS ACL prompt recurs despite the chain~~ | **Resolved by Spike A** — the CLI path does not prompt. Residual risk: if Apple ever gates the `security` CLI, fall back to Developer ID ($99/yr), which fixes both the prompt and Gatekeeper |
| Tauri `Image` clips at Retina scale | Spike B. Most likely place to lose a day |
| Windows 16 px illegibility | Accepted by decision; tooltip carries detail |
| Wayland popover cannot anchor | Accepted; Linux uses a centred window |
| Unsigned installers trigger Gatekeeper / SmartScreen | Document the bypass (`xattr -cr`, "More info → Run anyway") in the README |

**The ToS position is unchanged.** Per the README notice, this app violates
Anthropic's Consumer ToS. Producing `.msi` / `.deb` / `.dmg` installers increases
the temptation to distribute — **keep it unreleased.** Polling cost also
multiplies: three machines at 60 s is 3× the billed requests.
