// ClaudeUsageWidget.swift — WidgetKit extension
// Reads from the App Group container written by UsageStore.writeToAppGroup().
// Small widget: session % only.
// Medium widget: session + weekly side-by-side.

import WidgetKit
import SwiftUI

// MARK: - Timeline entry

struct UsageEntry: TimelineEntry {
    let date:           Date
    let sessionPercent: Double
    let weeklyPercent:  Double
    let sessionReset:   Date?
    let weeklyReset:    Date?
    let isStale:        Bool
}

// MARK: - Timeline provider

struct UsageProvider: TimelineProvider {
    private let suiteName = "group.com.you.claudeusage"

    func placeholder(in context: Context) -> UsageEntry {
        UsageEntry(date: Date(), sessionPercent: 72, weeklyPercent: 41,
                   sessionReset: nil, weeklyReset: nil, isStale: false)
    }

    func getSnapshot(in context: Context, completion: @escaping (UsageEntry) -> Void) {
        completion(readEntry())
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<UsageEntry>) -> Void) {
        let entry    = readEntry()
        // Refresh every 15 minutes — WidgetKit's minimum recommended interval
        let nextDate = Calendar.current.date(byAdding: .minute, value: 15, to: Date())!
        completion(Timeline(entries: [entry], policy: .after(nextDate)))
    }

    private func readEntry() -> UsageEntry {
        let defaults = UserDefaults(suiteName: suiteName)
        let session  = defaults?.double(forKey: "sessionPercent") ?? 0
        let weekly   = defaults?.double(forKey: "weeklyPercent")  ?? 0
        let sReset   = defaults?.double(forKey: "sessionResetAt").map { Date(timeIntervalSince1970: $0) }
        let wReset   = defaults?.double(forKey: "weeklyResetAt").map  { Date(timeIntervalSince1970: $0) }
        let updated  = defaults?.double(forKey: "lastUpdated").map    { Date(timeIntervalSince1970: $0) }
        let isStale  = updated.map { Date().timeIntervalSince($0) > 300 } ?? true  // stale after 5 min

        return UsageEntry(
            date: Date(), sessionPercent: session, weeklyPercent: weekly,
            sessionReset: sReset as? Date, weeklyReset: wReset as? Date,
            isStale: isStale
        )
    }
}

// MARK: - Small widget view (session only)

struct SmallWidgetView: View {
    let entry: UsageEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Claude", systemImage: "waveform.and.sparkles")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)

            Spacer()

            Text(String(format: "%.0f%%", entry.sessionPercent))
                .font(.system(size: 36, weight: .bold, design: .rounded))
                .foregroundStyle(entry.sessionPercent.usageColor)

            // Mini bar
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.secondary.opacity(0.2)).frame(height: 4)
                    Capsule()
                        .fill(entry.sessionPercent.usageColor)
                        .frame(width: geo.size.width * (entry.sessionPercent / 100), height: 4)
                }
            }
            .frame(height: 4)

            Text("session")
                .font(.system(size: 10))
                .foregroundStyle(.secondary)
        }
        .padding(14)
        .containerBackground(.clear, for: .widget)  // lets Liquid Glass background show through
    }
}

// MARK: - Medium widget view (session + weekly)

struct MediumWidgetView: View {
    let entry: UsageEntry

    var body: some View {
        HStack(spacing: 16) {
            barColumn(
                label:  "Session",
                value:  entry.sessionPercent,
                reset:  entry.sessionReset
            )

            Divider().frame(maxHeight: .infinity).opacity(0.25)

            barColumn(
                label:  "Weekly",
                value:  entry.weeklyPercent,
                reset:  entry.weeklyReset
            )
        }
        .padding(16)
        .containerBackground(.clear, for: .widget)
    }

    @ViewBuilder
    private func barColumn(label: String, value: Double, reset: Date?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)

            Text(String(format: "%.0f%%", value))
                .font(.system(size: 28, weight: .bold, design: .rounded))
                .foregroundStyle(value.usageColor)

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.secondary.opacity(0.2)).frame(height: 5)
                    Capsule()
                        .fill(value.usageColor)
                        .frame(width: geo.size.width * (value / 100), height: 5)
                }
            }
            .frame(height: 5)

            if let reset {
                Text(reset, style: .relative)
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
                    + Text(" left")
                    .font(.system(size: 9))
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Widget configuration

struct ClaudeUsageWidget: Widget {
    let kind = "ClaudeUsageWidget"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: UsageProvider()) { entry in
            Group {
                // WidgetKit picks the right view for the chosen size
            }
            // Liquid Glass widget background — iOS 26 automatically applies glass
            // to widgets with .containerBackground(.clear, for: .widget)
        }
        .configurationDisplayName("Claude Usage")
        .description("Track your Claude session and weekly quota at a glance.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

@main
struct ClaudeUsageWidgetBundle: WidgetBundle {
    var body: some Widget {
        ClaudeUsageWidget()
    }
}
