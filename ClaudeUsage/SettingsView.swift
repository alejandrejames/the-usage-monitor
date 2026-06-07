// SettingsView.swift — native macOS grouped Settings form

import SwiftUI
import UserNotifications

struct SettingsView: View {
    let authManager: AuthManager
    @Environment(\.dismiss) private var dismiss

    @AppStorage("refreshInterval")   private var refreshInterval   = 60
    @AppStorage("menuBarDisplay")    private var menuBarDisplay    = MenuBarDisplay.session.rawValue
    @AppStorage("alertThreshold80")  private var alertThreshold80  = true
    @AppStorage("alertThreshold95")  private var alertThreshold95  = true
    @State private var notificationsGranted = false

    var body: some View {
        Form {
            // ── Account ───────────────────────────────────────────────────
            Section("Account") {
                LabeledContent("Claude Code credential") {
                    HStack(spacing: 8) {
                        statusBadge
                        Button("Re-check") { authManager.refreshAvailability() }
                    }
                }
            }

            // ── Menu bar ──────────────────────────────────────────────────
            Section("Menu bar") {
                Picker("Display", selection: $menuBarDisplay) {
                    ForEach(MenuBarDisplay.allCases) { mode in
                        Text(mode.title).tag(mode.rawValue)
                    }
                }
            }

            // ── Refresh ───────────────────────────────────────────────────
            Section("Refresh") {
                Picker("Polling interval", selection: $refreshInterval) {
                    Text("30 seconds").tag(30)
                    Text("60 seconds").tag(60)
                    Text("2 minutes").tag(120)
                    Text("5 minutes").tag(300)
                }
            }

            // ── Alerts ────────────────────────────────────────────────────
            Section("Usage alerts") {
                Toggle("Alert at 80%", isOn: $alertThreshold80)
                Toggle("Alert at 95%", isOn: $alertThreshold95)

                if !notificationsGranted {
                    LabeledContent("Notifications are off") {
                        Button("Enable…") { requestNotifications() }
                    }
                }
            }

            // ── App ───────────────────────────────────────────────────────
            Section {
                Button("Quit Claude Usage", role: .destructive) {
                    NSApp.terminate(nil)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 380, height: 420)
        .navigationTitle("Settings")
        .onAppear { checkNotificationStatus() }
    }

    // MARK: - Account status badge

    @ViewBuilder
    private var statusBadge: some View {
        if authManager.isAuthenticated {
            let plan = authManager.subscriptionPlan.map { " · \($0.capitalized)" } ?? ""
            Label("Connected\(plan)", systemImage: "checkmark.circle.fill")
                .labelStyle(.titleAndIcon)
                .font(.system(size: 12))
                .foregroundStyle(.green)
        } else {
            Label("Not found", systemImage: "exclamationmark.triangle.fill")
                .labelStyle(.titleAndIcon)
                .font(.system(size: 12))
                .foregroundStyle(.orange)
        }
    }

    // MARK: - Notifications

    private func checkNotificationStatus() {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            DispatchQueue.main.async {
                notificationsGranted = settings.authorizationStatus == .authorized
            }
        }
    }

    private func requestNotifications() {
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound]) { granted, _ in
                DispatchQueue.main.async { notificationsGranted = granted }
            }
    }
}
