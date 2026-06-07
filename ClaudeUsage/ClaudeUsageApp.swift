// ClaudeUsageApp.swift — macOS app entry point
// Uses MenuBarExtra (SwiftUI native) so the app lives entirely in the menu bar.

import SwiftUI
import AppKit

@main
struct ClaudeUsageApp: App {

    @State private var authManager = AuthManager()
    @State private var store       = UsageStore()
    private var poller: UsagePoller

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
        MenuBarExtra {
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
            SettingsView(authManager: authManager)
        }
        .windowResizability(.contentSize)
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

    var body: some View {
        Group {
            if !isAuthenticated {
                Image(systemName: "chart.bar.fill")
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(.secondary)
            } else {
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
                        weekly:  store.weeklyPercent))
                }
            }
        }
        .opacity(store.isStale ? 0.5 : 1)
    }

    /// One icon + percentage row, coloured by usage level (single-line modes).
    @ViewBuilder
    private func metric(icon: String, percent: Double, size: CGFloat = 12) -> some View {
        HStack(spacing: 3) {
            Image(systemName: icon)
                .symbolRenderingMode(.hierarchical)
                .font(.system(size: size))
                .foregroundStyle(percent.usageColor)
            Text(String(format: "%.0f%%", percent))
                .font(.system(size: size, weight: .semibold, design: .rounded))
                .foregroundStyle(percent.usageColor)
        }
    }

    // MARK: - Two-line menu-bar image

    /// Draws two icon+percentage rows stacked vertically into an NSImage that
    /// fits the menu bar's height. Each row is tinted by its usage colour.
    private static func stackedImage(session: Double, weekly: Double) -> NSImage {
        let rowFont   = NSFont.systemFont(ofSize: 9, weight: .semibold)
        let iconSize: CGFloat = 9
        let rowHeight: CGFloat = 11
        let spacing:   CGFloat = 1
        let totalHeight = rowHeight * 2 + spacing

        // Measure widest row so the image is wide enough for both.
        func rowWidth(_ percent: Double) -> CGFloat {
            let text = String(format: " %.0f%%", percent)
            let textW = (text as NSString).size(withAttributes: [.font: rowFont]).width
            return iconSize + 3 + textW
        }
        let width = ceil(max(rowWidth(session), rowWidth(weekly))) + 2

        let image = NSImage(size: NSSize(width: width, height: totalHeight))
        image.lockFocus()

        func tintedSymbol(_ name: String, color: NSColor) -> NSImage? {
            let config = NSImage.SymbolConfiguration(pointSize: iconSize, weight: .semibold)
            guard let base = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
                .withSymbolConfiguration(config) else { return nil }
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
            let color = NSColor(percent.usageColor)
            if let symbol = tintedSymbol(icon, color: color) {
                let h = symbol.size.height
                symbol.draw(at: NSPoint(x: 1, y: y + (rowHeight - h) / 2), from: .zero,
                            operation: .sourceOver, fraction: 1)
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
