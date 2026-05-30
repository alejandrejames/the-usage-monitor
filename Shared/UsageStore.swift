// UsageStore.swift — Observable usage data store
// UsagePoller.swift — polling of the Anthropic usage endpoint + threshold alerts

import Foundation
import UserNotifications

// MARK: - Response model

struct UsageResponse: Codable {
    struct Limits: Codable {
        let sessionUsagePercent:  Double
        let weeklyUsagePercent:   Double
        let sessionResetAt:       Date
        let weeklyResetAt:        Date
    }
    let limits: Limits
}

// MARK: - UsageStore

@Observable
final class UsageStore {
    var sessionPercent:  Double = 0
    var weeklyPercent:   Double = 0
    var sessionResetAt:  Date?  = nil
    var weeklyResetAt:   Date?  = nil
    var lastUpdated:     Date?  = nil
    var isStale:         Bool   = false     // true if last poll failed

    // App Group suite name — must match WidgetKit target entitlement
    private let suiteName = "group.com.you.claudeusage"

    func update(from response: UsageResponse) {
        let l = response.limits
        let previousSession = sessionPercent
        sessionPercent = l.sessionUsagePercent
        weeklyPercent  = l.weeklyUsagePercent
        sessionResetAt = l.sessionResetAt
        weeklyResetAt  = l.weeklyResetAt
        lastUpdated    = Date()
        isStale        = false
        writeToAppGroup()
        AlertNotifier.checkThresholds(previous: previousSession, current: sessionPercent)
    }

    func markStale() { isStale = true }

    // Write display-safe (non-secret) data so the iOS widget can read it
    private func writeToAppGroup() {
        guard let defaults = UserDefaults(suiteName: suiteName) else { return }
        defaults.set(sessionPercent,               forKey: "sessionPercent")
        defaults.set(weeklyPercent,                forKey: "weeklyPercent")
        defaults.set(sessionResetAt?.timeIntervalSince1970,  forKey: "sessionResetAt")
        defaults.set(weeklyResetAt?.timeIntervalSince1970,   forKey: "weeklyResetAt")
        defaults.set(Date().timeIntervalSince1970,           forKey: "lastUpdated")
    }

    // Formatted reset countdowns
    var sessionResetString: String { resetString(for: sessionResetAt) }
    var weeklyResetString:  String { resetString(for: weeklyResetAt) }

    private func resetString(for date: Date?) -> String {
        guard let date else { return "—" }
        let diff = date.timeIntervalSinceNow
        if diff <= 0 { return "resetting…" }
        let h = Int(diff / 3600)
        let m = Int((diff.truncatingRemainder(dividingBy: 3600)) / 60)
        return h > 0 ? "resets in \(h)h \(m)m" : "resets in \(m)m"
    }
}

// MARK: - UsagePoller

final class UsagePoller {
    private let store:       UsageStore
    private let authManager: AuthManager
    private var timer:       Timer?

    // Endpoint: Anthropic's internal subscription usage endpoint
    // User-Agent must match Claude Code to avoid aggressive rate-limiting (429s)
    private let endpoint   = URL(string: "https://api.claude.ai/api/oauth/usage")!
    private let userAgent  = "claude-code/1.0.0"

    // Poll cadence is user-configurable via @AppStorage("refreshInterval").
    private var interval: TimeInterval {
        let stored = UserDefaults.standard.integer(forKey: "refreshInterval")
        return stored > 0 ? TimeInterval(stored) : 60
    }

    init(store: UsageStore, authManager: AuthManager) {
        self.store       = store
        self.authManager = authManager
    }

    func start() {
        poll()
        scheduleTimer()
    }

    private func scheduleTimer() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }

    func stop() { timer?.invalidate(); timer = nil }

    /// Triggers an immediate poll and re-aligns the timer to the current interval.
    func refreshNow() {
        poll()
        if timer != nil { scheduleTimer() }
    }

    private func poll() {
        guard let sessionKey = authManager.sessionKey else { return }

        var request = URLRequest(url: endpoint)
        request.setValue("sessionKey=\(sessionKey)", forHTTPHeaderField: "Cookie")
        request.setValue(userAgent,                   forHTTPHeaderField: "User-Agent")
        request.setValue("application/json",          forHTTPHeaderField: "Accept")

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601

        URLSession.shared.dataTask(with: request) { [weak self] data, _, error in
            guard let self else { return }
            guard let data, error == nil,
                  let response = try? decoder.decode(UsageResponse.self, from: data) else {
                DispatchQueue.main.async { self.store.markStale() }
                return
            }
            DispatchQueue.main.async { self.store.update(from: response) }
        }.resume()
    }
}

// MARK: - AlertNotifier

/// Fires local notifications when session usage crosses the 80% / 95%
/// thresholds upward. Edge-triggered so a single crossing alerts once,
/// not on every poll. Respects the @AppStorage toggles in SettingsView.
enum AlertNotifier {

    static func checkThresholds(previous: Double, current: Double) {
        let defaults = UserDefaults.standard
        // Toggles default to true when never set (matches SettingsView defaults).
        let alert80 = defaults.object(forKey: "alertThreshold80") as? Bool ?? true
        let alert95 = defaults.object(forKey: "alertThreshold95") as? Bool ?? true

        if alert95, crossed(95, previous: previous, current: current) {
            send(title: "Claude usage at 95%",
                 body: "You've used 95% of your session quota.")
        } else if alert80, crossed(80, previous: previous, current: current) {
            send(title: "Claude usage at 80%",
                 body: "You've used 80% of your session quota.")
        }
    }

    private static func crossed(_ threshold: Double, previous: Double, current: Double) -> Bool {
        previous < threshold && current >= threshold
    }

    private static func send(title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body  = body
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content:    content,
            trigger:    nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
