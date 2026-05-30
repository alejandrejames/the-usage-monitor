// PopoverView.swift — Liquid Glass usage panel (shown on menu bar click)
//
// Layout
// ┌──────────────────────────────┐
// │  header: title + status dot  │  ← .glassEffect() on whole panel
// │  ──────────────────────────  │
// │  Session (5 h)   72%  ████░  │
// │  resets in 1h 24m            │
// │                              │
// │  Weekly          41%  ███░░  │
// │  resets Mon 00:00            │
// │  ──────────────────────────  │
// │  [Refresh]        [Settings] │  ← .buttonStyle(.glass)
// └──────────────────────────────┘

import SwiftUI

struct PopoverView: View {
    let store:       UsageStore
    let authManager: AuthManager
    let poller:      UsagePoller

    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(spacing: 0) {

            // ── Header ────────────────────────────────────────────────────
            HStack(spacing: 8) {
                Circle()
                    .fill(store.isStale ? Color.secondary : store.sessionPercent.usageColor)
                    .frame(width: 8, height: 8)
                Text("Claude usage")
                    .font(.system(size: 13, weight: .semibold))
                Spacer()
                if let updated = store.lastUpdated {
                    Text(updated, style: .relative)
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                    Text("ago")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 16)
            .padding(.bottom, 12)

            Divider().opacity(0.25)

            // ── Usage bars ────────────────────────────────────────────────
            VStack(spacing: 14) {
                UsageBar(
                    value:      store.sessionPercent,
                    label:      "Session (5 h window)",
                    resetLabel: store.sessionResetString
                )

                UsageBar(
                    value:      store.weeklyPercent,
                    label:      "Weekly",
                    resetLabel: store.weeklyResetString
                )
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)

            Divider().opacity(0.25)

            // ── Action row ────────────────────────────────────────────────
            HStack(spacing: 8) {
                Button {
                    poller.refreshNow()
                } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                        .font(.system(size: 12))
                }
                .buttonStyle(.glass)

                Spacer()

                Button {
                    // Settings is a real Window scene (not a sheet) so it keeps
                    // its own focus and doesn't dismiss the menu-bar panel.
                    openWindow(id: "settings")
                    NSApp.activate(ignoringOtherApps: true)
                } label: {
                    Label("Settings", systemImage: "gear")
                        .font(.system(size: 12))
                }
                .buttonStyle(.glass)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
        .frame(width: 280)
        // ── Liquid Glass applied to the whole panel ───────────────────────
        .claudeGlass()
    }
}
