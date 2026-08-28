// ClaudeUsageApp.swift — macOS app entry point
// Uses MenuBarExtra (SwiftUI native) so the app lives entirely in the menu bar.

import SwiftUI
import AppKit

@main
struct ClaudeUsageApp: App {

    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    @State private var authManager = AuthManager()
    @State private var store       = UsageStore()
    private var poller: UsagePoller

    // When false, the menu-bar item is hidden and the widget is the only surface.
    // Re-launching the app opens Settings, so it's never truly unreachable.
    //
    // `isInserted` is driven by this @State (not @AppStorage directly): the
    // SwiftUI MenuBarExtra(isInserted:) binding only re-evaluates reliably from
    // App-owned @State. We seed it from UserDefaults and keep it in sync with an
    // observer so the Settings toggle takes effect immediately.
    @State private var showMenuBarIcon: Bool =
        UserDefaults.standard.object(forKey: "showMenuBarIcon") as? Bool ?? true

    init() {
        let auth  = AuthManager()
        let store = UsageStore()
        let poller = UsagePoller(store: store, authManager: auth)
        self.poller = poller
        _authManager = State(initialValue: auth)
        _store       = State(initialValue: store)

        // Start polling at launch (not when the popover opens) so the menu-bar
        // percentage is populated immediately. The poller no-ops without a token.
        if auth.isAuthenticated {
            DispatchQueue.main.async { poller.start() }
        }
    }

    var body: some Scene {
        // ── Menu bar icon ──────────────────────────────────────────────────
        // `isInserted` toggles the item's presence; bound to App @State which is
        // kept in sync with the "showMenuBarIcon" preference (see onChange below).
        MenuBarExtra(isInserted: $showMenuBarIcon) {
            // The popover that appears on click is itself Liquid Glass-styled
            if authManager.isAuthenticated {
                PopoverView(store: store, authManager: authManager, poller: poller)
            } else {
                NoCredentialView(authManager: authManager)
                    // If the token shows up later, begin polling.
                    .onChange(of: authManager.isAuthenticated) { _, ok in
                        if ok { poller.start() }
                    }
            }
        } label: {
            MenuBarLabel(store: store, isAuthenticated: authManager.isAuthenticated)
        }
        .menuBarExtraStyle(.window)

        // ── Settings window ────────────────────────────────────────────────
        // A standalone Window (not a sheet) so changing a control doesn't
        // resign focus and dismiss the menu-bar panel.
        Window("Claude Usage Settings", id: "settings") {
            SettingsView(authManager: authManager,
                         showMenuBarIcon: $showMenuBarIcon)
                .onAppear { NSApp.activate(ignoringOtherApps: true) }
        }
        .windowResizability(.contentSize)
    }
}

// MARK: - App delegate (lifecycle + open Settings on launch / re-launch)

/// Manages app lifecycle. The app must keep running with no windows open (it
/// lives in the menu bar / as a widget source), and only quit via the Settings
/// "Quit" button. Also resurfaces Settings on launch and re-launch — the escape
/// hatch when the menu-bar icon is hidden.
final class AppDelegate: NSObject, NSApplicationDelegate {

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Only auto-open Settings when the menu-bar icon is hidden — otherwise the
        // icon is the entry point and popping Settings on every launch is noise.
        // (Re-launching while running always opens it; see shouldHandleReopen.)
        let iconVisible = UserDefaults.standard.object(forKey: "showMenuBarIcon") as? Bool ?? true
        guard !iconVisible else { return }
        DispatchQueue.main.async { Self.openSettings() }
    }

    // Closing the Settings window must NOT quit the app — it keeps running in
    // the menu bar / as the widget's data source. Quit is explicit (Settings).
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    // Fired when the app is launched again while already running (e.g. from
    // Finder/Dock) — reopen Settings so the user can re-enable the menu bar.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        Self.openSettings()
        return true
    }

    static func openSettings() {
        NSApp.activate(ignoringOtherApps: true)
        // macOS 14+ uses the "Settings…" action; fall back to the older selector.
        if !NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil) {
            NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
        }
    }
}

// MARK: - Menu bar display mode

/// What the menu-bar icon shows. Persisted via @AppStorage("menuBarDisplay").
enum MenuBarDisplay: String, CaseIterable, Identifiable {
    case session
    case weekly
    case both

    var id: String { rawValue }

    var title: String {
        switch self {
        case .session: return "Session only"
        case .weekly:  return "Weekly only"
        case .both:    return "Session + Weekly"
        }
    }
}

// MARK: - Menu bar label

struct MenuBarLabel: View {
    let store: UsageStore
    let isAuthenticated: Bool

    @AppStorage("menuBarDisplay") private var displayRaw = MenuBarDisplay.session.rawValue
    private var display: MenuBarDisplay { MenuBarDisplay(rawValue: displayRaw) ?? .session }

    /// True when there's no usable reading: no Claude Code token, or the last
    /// poll failed (offline / API unreachable). The icons stay put; only the
    /// percentage is replaced by a "cannot connect" glyph.
    private var isDisconnected: Bool { !isAuthenticated || store.isStale }

    var body: some View {
        Group {
            switch display {
            case .session:
                metric(icon: "timer", percent: store.sessionPercent)
            case .weekly:
                metric(icon: "calendar", percent: store.weeklyPercent)
            case .both:
                // SwiftUI clips a multi-line label to one status-bar row, so
                // the second line vanishes. Render both rows into an NSImage
                // sized to the status bar height instead — guarantees both fit.
                Image(nsImage: Self.stackedImage(
                    session: store.sessionPercent,
                    weekly:  store.weeklyPercent,
                    disconnected: isDisconnected))
            }
        }
    }

    /// One icon + percentage row, coloured by usage level (single-line modes).
    /// When disconnected the icon keeps its place and the number is swapped for
    /// a slashed-wifi glyph — a faded percentage would read as a real (low) value.
    @ViewBuilder
    private func metric(icon: String, percent: Double, size: CGFloat = 12) -> some View {
        let tint: Color = isDisconnected ? .secondary : percent.usageColor
        HStack(spacing: 3) {
            Image(systemName: icon)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: size))
                .foregroundStyle(tint)
            if isDisconnected {
                Image(systemName: Self.disconnectedSymbol)
                    .symbolRenderingMode(.hierarchical)
                    .font(.system(size: size))
                    .foregroundStyle(tint)
            } else {
                Text(String(format: "%.0f%%", percent))
                    .font(.system(size: size, weight: .semibold, design: .rounded))
                    .foregroundStyle(tint)
            }
        }
    }

    /// Glyph that stands in for the percentage when there is no reading.
    fileprivate static let disconnectedSymbol = "wifi.slash"

    // MARK: - Two-line menu-bar image

    /// Draws two icon+percentage rows stacked vertically into an NSImage that
    /// fits the menu bar's height. Each row is tinted by its usage colour.
    private static func stackedImage(session: Double,
                                     weekly: Double,
                                     disconnected: Bool) -> NSImage {
        let rowFont   = NSFont.systemFont(ofSize: 9, weight: .semibold)
        let iconSize: CGFloat = 9
        let rowHeight: CGFloat = 11
        let spacing:   CGFloat = 1
        let totalHeight = rowHeight * 2 + spacing

        let symbolConfig = NSImage.SymbolConfiguration(pointSize: iconSize, weight: .semibold)

        /// Rendered width of an SF Symbol at the row's point size.
        func symbolWidth(_ name: String) -> CGFloat {
            NSImage(systemSymbolName: name, accessibilityDescription: nil)?
                .withSymbolConfiguration(symbolConfig)?.size.width ?? iconSize
        }

        // Measure widest row so the image is wide enough for both. When
        // disconnected each row is two symbols wide instead of icon + text.
        func rowWidth(_ percent: Double) -> CGFloat {
            if disconnected { return iconSize + 3 + symbolWidth(disconnectedSymbol) }
            let text = String(format: " %.0f%%", percent)
            let textW = (text as NSString).size(withAttributes: [.font: rowFont]).width
            return iconSize + 3 + textW
        }
        let width = ceil(max(rowWidth(session), rowWidth(weekly))) + 2

        let image = NSImage(size: NSSize(width: width, height: totalHeight))
        image.lockFocus()

        func tintedSymbol(_ name: String, color: NSColor) -> NSImage? {
            guard let base = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
                .withSymbolConfiguration(symbolConfig) else { return nil }
            let size = base.size
            let out = NSImage(size: size)
            out.lockFocus()
            base.draw(in: NSRect(origin: .zero, size: size))
            color.set()
            NSRect(origin: .zero, size: size).fill(using: .sourceAtop)
            out.unlockFocus()
            return out
        }

        func drawRow(icon: String, percent: Double, y: CGFloat) {
            let color = disconnected ? NSColor.secondaryLabelColor : NSColor(percent.usageColor)
            if let symbol = tintedSymbol(icon, color: color) {
                let h = symbol.size.height
                symbol.draw(at: NSPoint(x: 1, y: y + (rowHeight - h) / 2), from: .zero,
                            operation: .sourceOver, fraction: 1)
            }
            // Disconnected: the percentage slot becomes a slashed-wifi glyph so
            // the timer/calendar icons stay recognisable in place.
            if disconnected {
                if let glyph = tintedSymbol(disconnectedSymbol, color: color) {
                    let h = glyph.size.height
                    glyph.draw(at: NSPoint(x: iconSize + 4, y: y + (rowHeight - h) / 2),
                               from: .zero, operation: .sourceOver, fraction: 1)
                }
                return
            }
            let text = String(format: "%.0f%%", percent)
            let attrs: [NSAttributedString.Key: Any] = [.font: rowFont, .foregroundColor: color]
            (text as NSString).draw(at: NSPoint(x: iconSize + 4, y: y), withAttributes: attrs)
        }

        // y origin is bottom-left; draw weekly (bottom) then session (top).
        drawRow(icon: "calendar", percent: weekly,  y: 0)
        drawRow(icon: "timer",    percent: session, y: rowHeight + spacing)

        image.unlockFocus()
        image.isTemplate = false   // keep our colours, don't let the bar tint it
        return image
    }
}

// MARK: - No-credential prompt (shown when Claude Code's token isn't available)

struct NoCredentialView: View {
    let authManager: AuthManager

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "key.slash")
                .font(.system(size: 28))
                .foregroundStyle(.secondary)
            Text("Claude Code not detected")
                .font(.headline)
            Text("Sign in with Claude Code (run `claude` and log in), then re-check.")
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Button("Re-check") { authManager.refreshAvailability() }
                .buttonStyle(.glass)        // Liquid Glass button
        }
        .padding(20)
        .frame(width: 240)
        .claudeGlass()
    }
}
