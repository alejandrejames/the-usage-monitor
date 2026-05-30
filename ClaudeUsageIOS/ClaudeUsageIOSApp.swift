// ClaudeUsageIOSApp.swift — iOS companion app entry point

import SwiftUI

@main
struct ClaudeUsageIOSApp: App {
    @State private var authManager = AuthManager()
    @State private var store       = UsageStore()
    private var poller: UsagePoller

    init() {
        let auth  = AuthManager()
        let store = UsageStore()
        self.poller = UsagePoller(store: store, authManager: auth)
        _authManager = State(initialValue: auth)
        _store       = State(initialValue: store)
    }

    var body: some Scene {
        WindowGroup {
            if authManager.isAuthenticated {
                IOSMainView(store: store, authManager: authManager)
                    .onAppear { poller.start() }
            } else {
                IOSLoginView(authManager: authManager)
            }
        }
    }
}

// MARK: - iOS main usage view

struct IOSMainView: View {
    let store:       UsageStore
    let authManager: AuthManager

    var body: some View {
        ZStack {
            // Wallpaper-style gradient background so glass effect has content to refract
            LinearGradient(
                colors: [Color(.systemIndigo).opacity(0.6), Color(.systemPurple).opacity(0.4)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            VStack(spacing: 20) {
                // App header
                HStack {
                    Label("Claude Usage", systemImage: "waveform.and.sparkles")
                        .font(.system(size: 17, weight: .semibold))
                    Spacer()
                    Button("Sign out", role: .destructive) { authManager.logout() }
                        .buttonStyle(.glass)
                        .font(.system(size: 13))
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)

                // Session card — Liquid Glass
                usageCard(
                    title:       "Session",
                    subtitle:    "5-hour window",
                    value:       store.sessionPercent,
                    resetLabel:  store.sessionResetString
                )

                // Weekly card — Liquid Glass
                usageCard(
                    title:       "Weekly",
                    subtitle:    "Resets every Monday",
                    value:       store.weeklyPercent,
                    resetLabel:  store.weeklyResetString
                )

                Spacer()

                if let updated = store.lastUpdated {
                    Text("Updated \(updated, style: .relative) ago")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .padding(.bottom, 8)
                }
            }
            .padding(.vertical, 16)
        }
    }

    @ViewBuilder
    private func usageCard(
        title: String, subtitle: String, value: Double, resetLabel: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.system(size: 15, weight: .semibold))
                    Text(subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Text(String(format: "%.0f%%", value))
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .foregroundStyle(value.usageColor)
            }

            // Progress track
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.white.opacity(0.15)).frame(height: 8)
                    Capsule()
                        .fill(value.usageColor)
                        .frame(width: geo.size.width * (value / 100), height: 8)
                        .animation(.spring(duration: 0.5), value: value)
                }
            }
            .frame(height: 8)

            Text(resetLabel)
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
        }
        .padding(18)
        .padding(.horizontal, 4)
        .claudeGlass()        // ← Liquid Glass on each card
        .padding(.horizontal, 20)
    }
}

// MARK: - iOS Login view (WebView sheet)

struct IOSLoginView: View {
    let authManager: AuthManager

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [Color(.systemIndigo).opacity(0.7), Color(.systemPurple).opacity(0.5)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            VStack(spacing: 24) {
                Image(systemName: "waveform.and.sparkles")
                    .font(.system(size: 48))
                    .foregroundStyle(.white)

                Text("Claude Usage")
                    .font(.system(size: 28, weight: .bold))
                    .foregroundStyle(.white)

                Text("Sign in with your claude.ai account to track your session and weekly usage.")
                    .font(.system(size: 15))
                    .foregroundStyle(.white.opacity(0.75))
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 32)

                // Opens a Safari-style sheet via WKWebView
                // (LoginView reused from macOS — swap NSViewRepresentable → UIViewRepresentable)
                NavigationLink("Sign in with Claude") {
                    LoginView(authManager: authManager)
                }
                .buttonStyle(.glass)
                .font(.system(size: 16, weight: .medium))
                .padding(.top, 8)
            }
            .padding(24)
        }
    }
}
