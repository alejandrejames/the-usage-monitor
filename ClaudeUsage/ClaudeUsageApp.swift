// ClaudeUsageApp.swift — macOS app entry point
// Uses MenuBarExtra (SwiftUI native) so the app lives entirely in the menu bar.

import SwiftUI

@main
struct ClaudeUsageApp: App {

    @State private var authManager = AuthManager()
    @State private var store       = UsageStore()
    @State private var showLogin   = false
    private var poller: UsagePoller

    init() {
        let auth  = AuthManager()
        let store = UsageStore()
        self.poller = UsagePoller(store: store, authManager: auth)
        _authManager = State(initialValue: auth)
        _store       = State(initialValue: store)
    }

    var body: some Scene {
        // ── Menu bar icon ──────────────────────────────────────────────────
        MenuBarExtra {
            // The popover that appears on click is itself Liquid Glass-styled
            if authManager.isAuthenticated {
                PopoverView(store: store, authManager: authManager, poller: poller)
                    // Polling runs for the whole authenticated session, not just
                    // while the popover is open, so the menu-bar % stays current.
                    .onAppear { poller.start() }
            } else {
                LoginPromptView(showLogin: $showLogin)
            }
        } label: {
            MenuBarLabel(store: store, isAuthenticated: authManager.isAuthenticated)
        }
        .menuBarExtraStyle(.window)

        // ── Login sheet (presented from LoginPromptView) ───────────────────
        WindowGroup("Sign in to Claude", id: "login") {
            LoginView(authManager: authManager)
                .frame(width: 400, height: 560)
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

// MARK: - Login prompt (inside popover when unauthenticated)

struct LoginPromptView: View {
    @Binding var showLogin: Bool

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "person.badge.key")
                .font(.system(size: 28))
                .foregroundStyle(.secondary)
            Text("Not signed in")
                .font(.headline)
            Button("Sign in with Claude…") { showLogin = true }
                .buttonStyle(.glass)        // Liquid Glass button
        }
        .padding(20)
        .frame(width: 220)
        .claudeGlass()
    }
}
