// Popover logic.
//
// Reset-time formatting lives here rather than in Rust: Intl.DateTimeFormat
// carries full locale data for free, where Rust would need the heavy `icu`
// crate. This is the JS counterpart of SharedUsage.swift's resetString.

// The UI is buildless, so it reaches Tauri through the window global rather
// than an npm import. That global only exists when `withGlobalTauri` is set in
// tauri.conf.json — without it this file throws on its first line and the
// popover renders as an empty window, which is exactly what it looked like.
const tauri = window.__TAURI__;
if (!tauri) {
  document.body.innerHTML =
    '<p style="padding:16px;font:13px system-ui">Tauri API unavailable — ' +
    'set <code>app.withGlobalTauri</code> in tauri.conf.json.</p>';
  throw new Error("window.__TAURI__ is undefined");
}
const { invoke } = tauri.core;
const { listen } = tauri.event;
const { getCurrentWindow, LogicalSize } = tauri.window;

// Thresholds must match Theme.swift / UsageLevel::for_percent.
function usageColor(percent) {
  if (percent < 70) return "var(--usage-green)";
  if (percent < 90) return "var(--usage-amber)";
  return "var(--usage-red)";
}

// "resets in 3h 44m (9:42 PM)".
//
// The 5-hour window always resets today or tonight, so the clock time alone is
// unambiguous. The 7-day window can land on any date, so it also needs the
// weekday and date — the same two styles SharedUsage.swift used.
function resetString(epochMs, style) {
  if (epochMs == null) return "—";

  const target = new Date(epochMs);
  const diffMs = epochMs - Date.now();
  if (diffMs <= 0) return "resetting…";

  const totalMinutes = Math.floor(diffMs / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  const countdown = hours > 0 ? `resets in ${hours}h ${minutes}m` : `resets in ${minutes}m`;

  const options =
    style === "dateAndTime"
      ? { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" }
      : { hour: "numeric", minute: "2-digit" };

  // Undefined locale = the system locale, so 12- vs 24-hour follows the OS.
  return `${countdown} (${new Intl.DateTimeFormat(undefined, options).format(target)})`;
}

function relativeTime(epochMs) {
  if (epochMs == null) return "—";
  const seconds = Math.floor((Date.now() - epochMs) / 1000);
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.floor(minutes / 60)}h ago`;
}

const el = (id) => document.getElementById(id);

let latest = null;

function render(snapshot) {
  latest = snapshot;

  const authed = snapshot.isAuthenticated;
  el("no-credential").hidden = authed;
  el("panel").hidden = !authed;
  if (!authed) return;

  el("plan").textContent = snapshot.plan ?? "";

  for (const [key, style] of [
    ["session", "timeOnly"],
    ["weekly", "dateAndTime"],
  ]) {
    const percent = snapshot[`${key}Percent`];
    const stale = snapshot.isStale;

    // A dimmed number would read as a real (low) value, so show a dash
    // instead — the same reasoning as the tray's disconnected placeholder.
    el(`${key}-value`).textContent = stale ? "—" : `${Math.round(percent)}%`;

    const bar = el(`${key}-bar`);
    bar.style.width = stale ? "0%" : `${Math.min(100, Math.max(0, percent))}%`;
    bar.style.backgroundColor = usageColor(percent);

    el(`${key}-reset`).textContent = stale
      ? "no connection"
      : resetString(snapshot[`${key}ResetAtMs`], style);
  }

  renderStatus(snapshot);
  el("last-updated").textContent = snapshot.isStale
    ? "offline"
    : `updated ${relativeTime(snapshot.lastUpdatedMs)}`;
}

// Worst-of summary, mirroring core's overall_health severity ordering.
function renderStatus(snapshot) {
  const services = snapshot.services ?? [];
  const list = el("status-list");
  list.innerHTML = "";

  const labels = {
    operational: "Operational",
    degraded: "Degraded",
    partialOutage: "Partial outage",
    majorOutage: "Major outage",
    maintenance: "Maintenance",
    unknown: "Unknown",
  };

  for (const service of services) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.textContent = service.name;
    const health = document.createElement("span");
    health.textContent = labels[service.health] ?? service.health;
    li.append(name, health);
    list.append(li);
  }

  const severity = {
    operational: 0,
    unknown: 1,
    maintenance: 2,
    degraded: 3,
    partialOutage: 4,
    majorOutage: 5,
  };
  const worst = services.reduce(
    (acc, s) => ((severity[s.health] ?? 1) > (severity[acc] ?? 1) ? s.health : acc),
    services.length ? "operational" : "unknown"
  );

  el("status-dot").className = `status-dot ${worst}`;
  el("status-summary").textContent =
    worst === "operational" ? "All systems operational" : labels[worst] ?? "Service status";
}

el("refresh").addEventListener("click", () => invoke("refresh_now"));
el("recheck").addEventListener("click", () => invoke("recheck_credentials"));

// Quit is in the tray's right-click menu too, but that is not discoverable —
// so both popover states carry a button.
for (const id of ["quit", "quit-nc"]) {
  el(id).addEventListener("click", () => invoke("quit_app"));
}

listen("usage://snapshot", (event) => render(event.payload));
invoke("get_snapshot").then(render);

// Shrink the window to whatever is actually rendered. The panel's height
// varies — the status accordion expands, and the no-credential state is much
// shorter than the usage panel — so a fixed height would leave dead space
// below the content.
let lastHeight = 0;
async function fitWindow() {
  const visible = document.querySelector("section:not([hidden])");
  if (!visible) return;
  // Body padding is not included in the section's own box.
  const style = getComputedStyle(document.body);
  const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  const height = Math.ceil(visible.getBoundingClientRect().height + padding);
  if (height > 0 && Math.abs(height - lastHeight) > 1) {
    lastHeight = height;
    try {
      await getCurrentWindow().setSize(new LogicalSize(320, height));
    } catch {
      // Resizing is a nicety; a denied permission must not break the panel.
    }
  }
}

// Re-fit after any render, and when the status accordion is toggled.
const fitSoon = () => requestAnimationFrame(fitWindow);
new MutationObserver(fitSoon).observe(document.body, {
  subtree: true,
  childList: true,
  attributes: true,
});
document.querySelector(".status")?.addEventListener("toggle", fitSoon);

// Keep the countdowns and "updated Nm ago" honest without re-polling.
setInterval(() => latest && render(latest), 30000);
