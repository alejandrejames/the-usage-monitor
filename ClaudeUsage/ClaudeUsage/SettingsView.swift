// SettingsView.swift — Settings panel with Liquid Glass card sections

import SwiftUI
import UserNotifications

struct SettingsView: View {
    let authManager: AuthManager
    @Environment(\.dismiss) private var dismiss

    @AppStorage("refreshInterval")   private var refreshInterval   = 60
    @AppStorage("alertThreshold80")  private var alertThreshold80  = true
    @AppStorage("alertThreshold95")  private var alertThreshold95  = true
    @State private var notificationsGranted = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {

                    // ── Account card ──────────────────────────────────────
                    sectionCard("Account", icon: "person.badge.key") {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("claude.ai session")
                                    .font(.system(size: 13, weight: .medium))
                                Text(authManager.isAuthenticated ? "Connected via WebView" : "Not signed in")
                                    .font(.system(size: 11))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if authManager.isAuthenticated {
                                Button("Sign out", role: .destructive) {
                                    authManager.logout()
                                    dismiss()
                                }
                                .buttonStyle(.glass)
                                .font(.system(size: 12))
                            }
                        }
                    }

                    // ── Polling card ──────────────────────────────────────
                    sectionCard("Polling interval", icon: "clock.arrow.2.circlepath") {
                        Picker("", selection: $refreshInterval) {
                            Text("30 s").tag(30)
                            Text("60 s").tag(60)
                            Text("2 min").tag(120)
                            Text("5 min").tag(300)
                        }
                        .pickerStyle(.segmented)
                    }

                    // ── Alerts card ───────────────────────────────────────
                    sectionCard("Usage alerts", icon: "bell.badge") {
                        Toggle("Alert at 80%", isOn: $alertThreshold80)
                            .toggleStyle(.switch)
                        Toggle("Alert at 95%", isOn: $alertThreshold95)
                            .toggleStyle(.switch)

                        if !notificationsGranted {
                            Button("Enable notifications") { requestNotifications() }
                                .buttonStyle(.glass)
                                .font(.system(size: 12))
                        }
                    }
                }
                .padding(16)
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.buttonStyle(.glass)
                }
            }
        }
        .frame(width: 360, height: 440)
        .onAppear { checkNotificationStatus() }
    }

    // MARK: - Section card builder (Liquid Glass)

    @ViewBuilder
    private func sectionCard<Content: View>(
        _ title: String, icon: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: icon)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)
            content()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .claudeGlass()   // ← Liquid Glass on each section card
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
