// ClaudeUsageWidget.swift — macOS desktop widget for Claude usage.
// Reads the App Group snapshot the menu-bar app writes; never networks.

import WidgetKit
import SwiftUI

// MARK: - Timeline entry

struct UsageEntry: TimelineEntry {
    let date: Date
    let usage: WidgetUsage?      // nil until the app has written a snapshot
}

// MARK: - Provider

struct UsageProvider: TimelineProvider {
    func placeholder(in context: Context) -> UsageEntry {
        UsageEntry(date: Date(), usage: WidgetUsage(
            sessionPercent: 42, weeklyPercent: 18,
            sessionResetAt: Date().addingTimeInterval(3 * 3600),
            weeklyResetAt:  Date().addingTimeInterval(120 * 3600),
            lastUpdated: Date(), plan: "pro", isStale: false))
    }

    func getSnapshot(in context: Context, completion: @escaping (UsageEntry) -> Void) {
        completion(UsageEntry(date: Date(), usage: SharedUsage.read()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<UsageEntry>) -> Void) {
        let entry = UsageEntry(date: Date(), usage: SharedUsage.read())
        // The app drives prompt reloads via WidgetCenter; this 15-minute policy
        // is only a fallback so countdowns don't go stale if the app is closed.
        let next = Calendar.current.date(byAdding: .minute, value: 15, to: Date()) ?? Date()
        completion(Timeline(entries: [entry], policy: .after(next)))
    }
}

// MARK: - Widget definition

struct ClaudeUsageWidget: Widget {
    let kind = "ClaudeUsageWidget"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: UsageProvider()) { entry in
            ClaudeUsageWidgetView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
        }
        .configurationDisplayName("Claude Usage")
        .description("Your Claude session and weekly usage at a glance.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

@main
struct ClaudeUsageWidgetBundle: WidgetBundle {
    var body: some Widget { ClaudeUsageWidget() }
}

// MARK: - Root view (family switch)

struct ClaudeUsageWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: UsageEntry

    var body: some View {
        if let usage = entry.usage {
            switch family {
            case .systemSmall:  SmallView(usage: usage)
            case .systemLarge:  LargeView(usage: usage)
            default:            MediumView(usage: usage)
            }
        } else {
            NoDataView()
        }
    }
}

// MARK: - Small

private struct SmallView: View {
    let usage: WidgetUsage

    var body: some View {
        VStack(spacing: 8) {
            Ring(percent: usage.sessionPercent, icon: "timer")
            Text("Session")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .opacity(usage.isStale ? 0.6 : 1)
    }
}

/// A circular progress ring tinted by usage level.
private struct Ring: View {
    let percent: Double
    let icon: String

    var body: some View {
        ZStack {
            Circle()
                .stroke(.white.opacity(0.12), lineWidth: 8)
            Circle()
                .trim(from: 0, to: percent / 100)
                .stroke(percent.usageColor, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                .rotationEffect(.degrees(-90))
            VStack(spacing: 1) {
                Image(systemName: icon)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                Text(String(format: "%.0f%%", percent))
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .foregroundStyle(percent.usageColor)
            }
        }
        .padding(4)
    }
}

// MARK: - Medium

private struct MediumView: View {
    let usage: WidgetUsage

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            UsageBar(value: usage.sessionPercent, label: "Session (5 h)",
                     resetLabel: usage.sessionResetString)
            UsageBar(value: usage.weeklyPercent, label: "Weekly (7 d)",
                     resetLabel: usage.weeklyResetString)
        }
        .opacity(usage.isStale ? 0.6 : 1)
    }

    private var header: some View {
        HStack {
            Text("Claude Usage")
                .font(.system(size: 13, weight: .semibold))
            Spacer()
            if usage.isStale {
                Image(systemName: "wifi.exclamationmark")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
            }
        }
    }
}

// MARK: - Large

private struct LargeView: View {
    let usage: WidgetUsage

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Claude Usage")
                    .font(.system(size: 15, weight: .semibold))
                Spacer()
                if let plan = usage.plan {
                    Text(plan.capitalized)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                }
            }

            UsageBar(value: usage.sessionPercent, label: "Session (5 h)",
                     resetLabel: usage.sessionResetString)
            UsageBar(value: usage.weeklyPercent, label: "Weekly (7 d)",
                     resetLabel: usage.weeklyResetString)

            Spacer()

            HStack(spacing: 4) {
                if usage.isStale {
                    Image(systemName: "wifi.exclamationmark")
                        .foregroundStyle(.orange)
                    Text("Disconnected")
                } else {
                    Text("Updated \(updatedString)")
                }
            }
            .font(.system(size: 10))
            .foregroundStyle(.tertiary)
        }
    }

    private var updatedString: String {
        guard let last = usage.lastUpdated else { return "—" }
        let mins = Int(-last.timeIntervalSinceNow / 60)
        if mins <= 0 { return "just now" }
        return mins == 1 ? "1m ago" : "\(mins)m ago"
    }
}

// MARK: - Empty state

private struct NoDataView: View {
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "chart.bar.xaxis")
                .font(.system(size: 24))
                .foregroundStyle(.secondary)
            Text("Open ClaudeUsage")
                .font(.system(size: 12, weight: .medium))
            Text("Launch the app to start tracking.")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding()
    }
}
