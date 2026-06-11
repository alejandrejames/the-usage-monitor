// SharedUsage.swift — App Group handoff between the menu-bar app and the widget.
// Only display-safe values cross this boundary; the OAuth token never does.

import Foundation
#if canImport(WidgetKit)
import WidgetKit
#endif

// MARK: - App Group

enum AppGroup {
    static let id = "group.com.you.claudeusage"
}

// MARK: - Shared usage snapshot

/// The display-safe usage values the widget reads. Codable so it can be stored
/// as JSON in the App Group's UserDefaults suite. Deliberately contains no token.
struct WidgetUsage: Codable, Equatable {
    var sessionPercent: Double
    var weeklyPercent:  Double
    var sessionResetAt: Date?
    var weeklyResetAt:  Date?
    var lastUpdated:    Date?
    var plan:           String?
    var isStale:        Bool

    /// True when the displayed data is identical, ignoring `lastUpdated` (which
    /// changes on every poll). Used to skip redundant widget writes/reloads.
    func sameDisplay(as other: WidgetUsage) -> Bool {
        sessionPercent == other.sessionPercent &&
        weeklyPercent  == other.weeklyPercent  &&
        sessionResetAt == other.sessionResetAt &&
        weeklyResetAt  == other.weeklyResetAt  &&
        plan           == other.plan           &&
        isStale        == other.isStale
    }

    /// "resets in 3h 44m" style countdown, shared by the app popover and widget.
    static func resetString(for date: Date?, now: Date = Date()) -> String {
        guard let date else { return "—" }
        let diff = date.timeIntervalSince(now)
        if diff <= 0 { return "resetting…" }
        let h = Int(diff / 3600)
        let m = Int((diff.truncatingRemainder(dividingBy: 3600)) / 60)
        return h > 0 ? "resets in \(h)h \(m)m" : "resets in \(m)m"
    }

    var sessionResetString: String { Self.resetString(for: sessionResetAt) }
    var weeklyResetString:  String { Self.resetString(for: weeklyResetAt) }
}

// MARK: - Read / write

enum SharedUsage {
    // Why not UserDefaults(suiteName:) or the App Group container URL?
    //   - The free Personal Team does NOT truly provision App Groups, so the
    //     sandboxed widget gets no group-container mapping —
    //     containerURL(forSecurityApplicationGroupIdentifier:) returns nil and
    //     UserDefaults(suiteName:) reads empty inside the widget.
    //   - WidgetKit extensions MUST stay sandboxed (an unsandboxed appex won't
    //     register), so we can't just read the global path from the widget.
    //
    // What works: the widget can always read its OWN sandbox Documents
    // directory, and the unsandboxed app (full filesystem access) can write
    // directly into that same physical path. So the handoff file lives in the
    // widget's container Documents. Only display-safe values are written — never
    // the token.
    private static let fileName = "usage.json"

    /// The handoff file as the UNSANDBOXED app sees it: an absolute path into the
    /// widget's sandbox container, built from the app's real home directory.
    /// Used only on the write side. (Inside the widget, `homeDirectoryForCurrentUser`
    /// is the container root, so this path would be wrong there — the widget uses
    /// `widgetLocalURL` instead.)
    private static var appWriteURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Containers/com.you.claudeusage.widget/Data/Documents", isDirectory: true)
            .appendingPathComponent(fileName)
    }

    /// The handoff file as the SANDBOXED widget sees it: its own Documents
    /// directory, which resolves to the same physical file the app wrote.
    private static var widgetReadURL: URL? {
        try? FileManager.default
            .url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: false)
            .appendingPathComponent(fileName)
    }

    /// Last snapshot we wrote, to skip redundant writes + widget reloads.
    private static var lastWritten: WidgetUsage?

    /// Persists the snapshot to the widget's container and refreshes the widget.
    /// Called from the (unsandboxed) app, which can write into the widget's box.
    /// No-ops when the displayed values are unchanged (ignoring `lastUpdated`), so
    /// polling an idle account doesn't burn WidgetKit's reload budget.
    static func write(_ usage: WidgetUsage) {
        if let last = lastWritten, last.sameDisplay(as: usage) { return }
        guard let data = try? JSONEncoder().encode(usage) else { return }
        lastWritten = usage
        let dir = appWriteURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try? data.write(to: appWriteURL, options: .atomic)
        #if canImport(WidgetKit)
        WidgetCenter.shared.reloadAllTimelines()
        #endif
    }

    /// Reads the last snapshot from the widget's own Documents directory, or nil
    /// if none yet. Only meaningful inside the widget process.
    static func read() -> WidgetUsage? {
        guard let url = widgetReadURL, let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(WidgetUsage.self, from: data)
    }
}
