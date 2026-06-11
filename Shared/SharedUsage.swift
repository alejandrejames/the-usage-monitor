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
struct WidgetUsage: Codable {
    var sessionPercent: Double
    var weeklyPercent:  Double
    var sessionResetAt: Date?
    var weeklyResetAt:  Date?
    var lastUpdated:    Date?
    var plan:           String?
    var isStale:        Bool

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

    /// The widget's sandbox container Documents path (deterministic from the
    /// widget bundle id). Both processes target this same absolute path.
    private static var fileURL: URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return home
            .appendingPathComponent("Library/Containers/com.you.claudeusage.widget/Data/Documents", isDirectory: true)
            .appendingPathComponent(fileName)
    }

    /// Persists the snapshot to the widget's container and refreshes the widget.
    /// Called from the (unsandboxed) app, which can write into the widget's box.
    static func write(_ usage: WidgetUsage) {
        guard let data = try? JSONEncoder().encode(usage) else { return }
        let dir = fileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try? data.write(to: fileURL, options: .atomic)
        #if canImport(WidgetKit)
        WidgetCenter.shared.reloadAllTimelines()
        #endif
    }

    /// Reads the last snapshot. From inside the widget, `homeDirectoryForCurrentUser`
    /// resolves to the sandbox container root, so this reads the same file.
    static func read() -> WidgetUsage? {
        // Inside the sandboxed widget, the home dir IS the container, so the
        // path above resolves to "<container>/Data/Documents/usage.json".
        // Try that first, then the app-side absolute path as a fallback.
        let candidates = [widgetLocalURL, fileURL]
        for url in candidates {
            if let url, let data = try? Data(contentsOf: url),
               let usage = try? JSONDecoder().decode(WidgetUsage.self, from: data) {
                return usage
            }
        }
        return nil
    }

    /// Inside the widget sandbox, the Documents directory resolves correctly via
    /// the standard API — no hardcoded path needed on the read side.
    private static var widgetLocalURL: URL? {
        try? FileManager.default
            .url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: false)
            .appendingPathComponent(fileName)
    }
}
