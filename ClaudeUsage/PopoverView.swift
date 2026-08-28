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
    let store:        UsageStore
    let authManager:  AuthManager
    let poller:       UsagePoller
    let statusStore:  StatusStore
    let statusPoller: StatusPoller

    @Environment(\.openWindow) private var openWindow

    // Collapsed by default — status is a glanceable summary; the per-service
    // rows are opt-in. Persisted so the panel reopens the way it was left.
    @AppStorage("statusExpanded") private var statusExpanded = false

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

            // ── Service status accordion ──────────────────────────────────
            StatusSection(store: statusStore, isExpanded: $statusExpanded)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)

            Divider().opacity(0.25)

            // ── Action row ────────────────────────────────────────────────
            HStack(spacing: 8) {
                Button {
                    poller.refreshNow()
                    statusPoller.refreshNow()
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


// MARK: - Service status accordion

/// Collapsible list of Claude service statuses (claude.ai, Claude Code), sourced
/// from status.claude.com. Collapsed it is a single summary row; the chevron
/// expands it into per-service rows.
private struct StatusSection: View {
    let store: StatusStore
    @Binding var isExpanded: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {

            // ── Summary row (always visible, toggles the accordion) ───────
            Button {
                withAnimation(.easeInOut(duration: 0.18)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))

                    Text("Service status")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.secondary)

                    Spacer()

                    Text(summaryLabel)
                        .font(.system(size: 11))
                        .foregroundStyle(summaryColor)
                    Circle()
                        .fill(summaryColor)
                        .frame(width: 7, height: 7)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Service status, \(summaryLabel)")
            .accessibilityHint(isExpanded ? "Collapse service list" : "Expand service list")

            // ── Per-service rows ─────────────────────────────────────────
            if isExpanded {
                VStack(alignment: .leading, spacing: 6) {
                    if store.services.isEmpty {
                        Text(store.isStale ? "Status unavailable" : "Checking…")
                            .font(.system(size: 11))
                            .foregroundStyle(.tertiary)
                    } else {
                        ForEach(store.services) { service in
                            HStack(spacing: 8) {
                                Circle()
                                    .fill(color(for: service.health))
                                    .frame(width: 6, height: 6)
                                Text(service.name)
                                    .font(.system(size: 11))
                                    .foregroundStyle(.secondary)
                                Spacer()
                                Text(service.health.label)
                                    .font(.system(size: 11))
                                    .foregroundStyle(color(for: service.health))
                            }
                        }
                    }
                }
                .padding(.leading, 17)      // align under the summary text
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }

    /// Collapsed summary: the worst state across tracked services.
    private var summaryLabel: String {
        if store.isStale && store.services.isEmpty { return "Unavailable" }
        if store.services.isEmpty                  { return "Checking…" }
        return store.allOperational ? "All systems normal" : store.overall.label
    }

    private var summaryColor: Color {
        if store.services.isEmpty { return .secondary }
        return color(for: store.overall)
    }

    private func color(for health: ServiceHealth) -> Color {
        switch health {
        case .operational:                 return Color("UsageGreen")
        case .degraded, .maintenance:      return Color("UsageAmber")
        case .partialOutage, .majorOutage: return Color("UsageRed")
        case .unknown:                     return .secondary
        }
    }
}
