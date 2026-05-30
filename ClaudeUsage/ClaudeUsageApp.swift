// ClaudeUsageApp.swift — macOS app entry point
// Uses MenuBarExtra (SwiftUI native) so the app lives entirely in the menu bar.

import SwiftUI

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

// MARK: - Menu bar label

struct MenuBarLabel: View {
    let store: UsageStore
    let isAuthenticated: Bool

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "chart.bar.fill")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(isAuthenticated ? store.sessionPercent.usageColor : .secondary)

            if isAuthenticated {
                Text(String(format: "%.0f%%", store.sessionPercent))
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(store.sessionPercent.usageColor)
            }
        }
        .opacity(store.isStale ? 0.5 : 1)
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
