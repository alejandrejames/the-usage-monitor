# Architecture

How ClaudeUsage is put together, and why. For the history of the port that
produced it, see [cross-platform.md](cross-platform.md).

---

## The shape of it

```
┌─ tray icon ────────────────┐   ┌─ popover (WebView) ──────────┐
│  ⏱ 43%   rendered bitmap   │   │  Session / Weekly bars       │
│  📅 69%   (macOS, Windows) │   │  service status              │
│          set_title (Linux) │   │  settings, refresh, quit     │
└────────────┬───────────────┘   └──────────────┬───────────────┘
             │                                  │
             │  tray::update()                  │  Tauri IPC
             │                                  │  (commands + events)
        ┌────┴──────────────────────────────────┴────┐
        │            crates/app  (the host)          │
        │  main.rs    tray, window, plugins, threads │
        │  state.rs   AppState + the poll loops      │
        │  settings   preferences, persisted as JSON │
        │  tray/      render, sizing, linux quirks   │
        └────────────────────┬───────────────────────┘
                             │  pure function calls
        ┌────────────────────┴───────────────────────┐
        │        crates/core  (no UI, no I/O)        │
        │  credentials  where the token lives per OS │
        │  usage        parse ratelimit headers      │
        │  status       parse Statuspage JSON        │
        │  model        thresholds, health ordering  │
        │  alerts       edge-triggered crossings     │
        └────────────────────┬───────────────────────┘
                             │  HTTP (ureq, blocking)
             ┌───────────────┴────────────────┐
             │  api.anthropic.com/v1/messages │
             │  status.claude.com/api/v2      │
             └────────────────────────────────┘
```

Two background threads do the work. A **usage poller** runs on the configured
interval; a **status poller** runs every 5 minutes, deliberately independent so
a failing status fetch never affects the usage reading. Both publish through
one `AppSnapshot`, which reaches the tray directly and the popover as an event.

---

## Why this stack

| Layer | Choice | Why not the obvious alternative |
|---|---|---|
| Core logic | Plain Rust, no deps beyond serde | Testable on every platform without a UI or a network |
| Shell | Tauri v2 | Electron is ~85 MB and ~1.4 s to start, for an app that idles all day. This is 8.8 MB |
| UI | Hand-written HTML/CSS/JS | No bundler, no `package.json`, no `node_modules`. Node is needed only by the Tauri CLI |
| HTTP | `ureq`, blocking | The pollers own their threads. An async runtime would earn nothing here |
| Tray text | `ab_glyph` | Two rows of digits need no shaping or bidi, so `cosmic-text` would be overkill |
| State | `Mutex<AppSnapshot>` | `@Observable` has no Rust equivalent; the UI is notified explicitly |

**93 unit tests** — 63 in core, 30 in the app — plus two probe binaries that
exercise the live path (`make probe`, `make poll`).

---

## The core crate

Everything here is a pure function of its inputs. It performs no I/O, touches
no UI, and compiles identically on all three platforms. The host does the
talking and hands results in.

### `usage` — the whole API contract

The app sends a **minimal 1-token `POST /v1/messages` and discards the
completion**. The usage lives in four response headers:

```
anthropic-ratelimit-unified-5h-utilization   0.43   (a fraction)
anthropic-ratelimit-unified-7d-utilization   0.69
anthropic-ratelimit-unified-5h-reset         1788…  (epoch seconds)
anthropic-ratelimit-unified-7d-reset         1788…
```

Three details that are easy to get wrong:

- **A 429 is a good response.** The headers ride along with it, so a
  rate-limited reply is still a valid reading and must not be treated as a
  failure.
- **Header lookup is case-insensitive.** `HTTPURLResponse` gave the Swift
  version this free; a `HashMap` keyed on wire casing would not.
- **`NaN` and `inf` parse fine as `f64`.** They are rejected explicitly, or
  they would reach the tray renderer and display as "NaN%".

Reset times are converted to **epoch milliseconds** and carried that way all
the way to the WebView, which formats them with `Intl.DateTimeFormat`. Rust
has no locale-aware date formatting without the heavy `icu` crate; the browser
already has the data.

### `credentials` — the only per-OS branch

| Platform | Location |
|---|---|
| macOS | login keychain, service `Claude Code-credentials` |
| Linux | `$CLAUDE_CONFIG_DIR/.credentials.json` else `~/.claude/.credentials.json` |
| Windows | `%USERPROFILE%\.claude\.credentials.json` |

macOS tries a **chain**: the file, then `/usr/bin/security`, then the native
Security framework. That order is measured, not assumed — see
[auth.md](auth.md) for the timings. The short version: the keychain item's
`partition_id` allows only `apple-tool:`, so the native API prompts and an
unsigned build cannot durably dismiss it.

The blob also carries OAuth tokens for **every MCP server the user has
authorised**, so nothing in the codebase logs it, a token, or any substring of
one.

### `model`, `alerts`, `status`

Thresholds are green `<70`, amber `<90`, red `≥90` — boundaries inclusive
upward, matching the Swift original exactly.

Service health uses a **severity ordering** rather than a hardcoded if-chain,
which buys one property the Swift version lacked: `Unknown` sorts above
`Operational` but below every real problem, so an unmatched component stays
visible without masking a live outage.

Alerts are **edge-triggered** — a crossing fires once, not on every poll while
usage stays high. A crossed-and-enabled 95% suppresses the 80%; a *disabled*
95% lets the 80% fire. Both behaviours are ported deliberately.

---

## The host crate

### Polling and backoff

A clean poll waits the configured interval. A failure backs off from a 10 s
floor, doubling up to the normal cadence. This replaces the Swift app's
`NWPathMonitor`, which re-polled the instant connectivity returned — there is
no clean cross-platform equivalent, and backoff recovers nearly as fast for a
fraction of the complexity.

### Credential caching

Every uncached read on macOS risks the ACL prompt, so the resolved credential
is held in memory and only re-read when missing or expired. A **10-second
refetch floor** stops a burst of 401s becoming a burst of prompts.

`force_refresh()` exists separately from `invalidate()` because the floor would
otherwise defeat the popover's Re-check button: `invalidate()` empties the
cache, and the floor then serves that empty cache as `NotFound` without
consulting a source. The floor is there to stop a storm, not to override
someone deliberately asking.

### Settings

There is no cross-platform `UserDefaults`, so preferences are JSON under the
OS config directory. Values are clamped on both load and save — a hand-edited
`0` interval would spin the poll loop — and the save command returns what was
*actually* stored, so the UI reflects clamping rather than silently disagreeing.

The interval is read per tick, so a change applies on the next cycle without
restarting the poller.

---

## The tray

The one genuinely divergent part. Each platform's constraints are different
and the differences are forced, not stylistic.

| | macOS | Windows | Linux |
|---|---|---|---|
| Numbers | in the bitmap | in the bitmap | **`set_title`** |
| Icon size | any (scaled to 18 pt) | **exact DPI size** | any |
| Tooltip | ✓ | ✓ | **unsupported** |
| Redraw cost | free | free | **writes a PNG to disk** |

**macOS scales, Windows does not.** The tao backend normalises the icon to a
fixed height in points and derives width from the aspect ratio, so an oversized
buffer is scaled down cleanly — the app renders at 108 px for sharpness.
Windows instead downscales an oversized icon *poorly*, so `tray::sizing` queries
`GetSystemMetricsForDpi(SM_CYSMICON, …)` per render.

**Linux redraws only on colour change.** The GTK/AppIndicator backend takes an
`icon_path`, not a buffer, so every `set_icon` writes a PNG into
`$XDG_RUNTIME_DIR`. At a 60 s poll that would be ~1,440 writes a day; keying
the redraw on the threshold bucket brings it to a handful.

### Rendering

Two rows of `NN%` with an icon each, composited into an RGBA buffer.

- **The font is bundled** (DejaVu Sans Bold, redistributable). All ten digits
  share one advance width — verified by measurement — so the icon does not
  jitter as usage changes. System font enumeration differs per platform and is
  absent in some Linux containers.
- **The row icons are cropped to their content box.** The bundled art carries
  different amounts of transparent margin (stopwatch ~90 % of canvas, calendar
  ~70 %), so scaling the full canvas drew one visibly smaller than the other.
- **Scaling is box-averaged.** Point-sampling 1024 px down to ~20 drops most of
  the strokes.
- **Text overshoots its row box** (1.18×), because digits and `%` have no
  descenders and the metrics reserve space for them anyway. A single row uses
  0.82× instead — it already owns the full height, and the same overshoot made
  the icon 3× wider than tall.

Width is the only real cost, since the menu bar fixes the height, so a test
bounds the aspect ratio in both modes.

---

## The popover

Plain HTML/CSS/JS in `ui/`, served by whatever WebView the OS provides.

`.glassEffect()` becomes `backdrop-filter`, which needs no OS-version fallback
the way Liquid Glass did. The window is **transparent and undecorated**, sized
to its content by JS — the panel's height varies with the settings and status
sections, and a fixed height left dead space.

**Tauri v2 denies all IPC unless a capability grants it.** `capabilities/`
allows `core:default`, event listen/unlisten, and window resize, scoped to the
`main` window. Without it `invoke()` and `listen()` are silently refused and
the popover renders its default state forever — which is exactly how it failed
once.

Positioning uses `tauri-plugin-positioner`'s `TrayBottomCenter`, **validated
rather than trusted**: if the result would land off-screen or straddle a
monitor edge, it falls back to the top-right of the active monitor.
`TrayCenter` needs a tray event before it knows where the icon is, and is
known-broken on macOS multi-monitor.

---

## Things that bite

Collected because each one cost real time:

- **`app.exit()` raises `ExitRequested`.** Blocking every one of those to stop
  the popover quitting the app also made the app unquittable.
- **A tray click after focus-loss arrives second.** Without a 250 ms debounce
  the click reopens the window the focus-loss just closed.
- **macOS needs `ActivationPolicy::Accessory`**, or a Dock icon appears and the
  app steals focus.
- **`withGlobalTauri` defaults to false.** A buildless UI reaching Tauri
  through `window.__TAURI__` gets `undefined` without it.
- **Wayland cannot position windows client-side.** The Linux popover is a
  centred window by design, not a bug.
- **GNOME has had no tray since 3.26**, and Bazzite ships GNOME. The app
  detects a missing `StatusNotifierWatcher` and prints distro-specific advice.

---

## What is verified, and what is not

| Platform | Build | Tests | Actually run |
|---|---|---|---|
| macOS | ✅ | ✅ 93 | ✅ in use |
| Linux | ✅ in Docker | ✅ 88 in container | ❌ never rendered |
| Windows | ⚠️ core only¹ | ❌ | ❌ |

¹ The app crate cannot be cross-compiled from macOS: `ring`, via
`ureq`→`rustls`, needs a C toolchain targeting Windows. The core crate and the
`cfg`-gated Windows branches inside it do check, which catches wrong Win32
symbols and bad feature flags — confirmed by deliberately breaking them and
watching the check fail.

**The shared logic is tested. The appearance is not, and is not identical by
design.** Expect Linux to look close minus the row icons, and Windows at 100 %
DPI to be cramped — 16 px for two rows is ~8 px each.
