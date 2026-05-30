// UsageStore.swift — Observable usage data store
// UsagePoller.swift — polling of the Anthropic usage endpoint + threshold alerts

import Foundation
import UserNotifications

// MARK: - Usage snapshot

/// Usage parsed from the `anthropic-ratelimit-unified-*` response headers that
/// accompany a normal /v1/messages call (the technique mature usage monitors
/// use). `5h` is the session window; `7d` is the weekly window.
struct UsageSnapshot {
    let sessionPercent: Double      // 0–100
    let weeklyPercent:  Double      // 0–100
    let sessionResetAt: Date?
    let weeklyResetAt:  Date?

    /// Builds a snapshot from HTTP response headers. The `*-utilization`
    /// headers are fractions (0–1); `*-reset` headers are epoch seconds.
    init?(headers: [AnyHashable: Any]) {
        func double(_ key: String) -> Double? {
            (headers[key] as? String).flatMap(Double.init)
        }
        guard let s = double("anthropic-ratelimit-unified-5h-utilization"),
              let w = double("anthropic-ratelimit-unified-7d-utilization") else { return nil }
        sessionPercent = (s * 100).rounded()    // headers are coarse; integer % is plenty
        weeklyPercent  = (w * 100).rounded()
        sessionResetAt = double("anthropic-ratelimit-unified-5h-reset").map { Date(timeIntervalSince1970: $0) }
        weeklyResetAt  = double("anthropic-ratelimit-unified-7d-reset").map { Date(timeIntervalSince1970: $0) }
    }
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

    func update(from snapshot: UsageSnapshot) {
        let previousSession = sessionPercent
        sessionPercent = snapshot.sessionPercent
        weeklyPercent  = snapshot.weeklyPercent
        sessionResetAt = snapshot.sessionResetAt
        weeklyResetAt  = snapshot.weeklyResetAt
        lastUpdated    = Date()
        isStale        = false
        AlertNotifier.checkThresholds(previous: previousSession, current: sessionPercent)
    }

    func markStale() { isStale = true }

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

    // We send a minimal (1-token) /v1/messages request and read usage from the
    // `anthropic-ratelimit-unified-*` response headers — the same technique
    // mature Claude usage monitors use, and more durable than /api/oauth/usage.
    private let endpoint = URL(string: "https://api.anthropic.com/v1/messages")!

    // Poll cadence is user-configurable via @AppStorage("refreshInterval").
    private var interval: TimeInterval {
        let stored = UserDefaults.standard.integer(forKey: "refreshInterval")
        return stored > 0 ? TimeInterval(stored) : 60
    }
    private var lastScheduledInterval: TimeInterval = 0
    private var defaultsObserver: NSObjectProtocol?

    init(store: UsageStore, authManager: AuthManager) {
        self.store       = store
        self.authManager = authManager
    }

    deinit {
        if let defaultsObserver { NotificationCenter.default.removeObserver(defaultsObserver) }
    }

    func start() {
        poll()
        scheduleTimer()
        observeIntervalChanges()
    }

    private func scheduleTimer() {
        timer?.invalidate()
        lastScheduledInterval = interval
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }

    // Reschedule live when the user changes the polling interval in Settings.
    private func observeIntervalChanges() {
        guard defaultsObserver == nil else { return }
        defaultsObserver = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            guard let self, self.timer != nil, self.interval != self.lastScheduledInterval else { return }
            self.scheduleTimer()
        }
    }

    func stop() { timer?.invalidate(); timer = nil }

    /// Triggers an immediate poll and re-aligns the timer to the current interval.
    func refreshNow() {
        poll()
        if timer != nil { scheduleTimer() }
    }

    // A minimal request body — the response's rate-limit headers carry the
    // usage data regardless of the (discarded) completion.
    private static let body = try! JSONSerialization.data(withJSONObject: [
        "model":      "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages":   [["role": "user", "content": "."]],
    ])

    private func poll() {
        guard let token = authManager.accessToken else { return }

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.httpBody   = Self.body
        request.setValue("Bearer \(token)",      forHTTPHeaderField: "Authorization")
        request.setValue("oauth-2025-04-20",     forHTTPHeaderField: "anthropic-beta")
        request.setValue("2023-06-01",           forHTTPHeaderField: "anthropic-version")
        request.setValue("application/json",     forHTTPHeaderField: "content-type")

        URLSession.shared.dataTask(with: request) { [weak self] _, urlResponse, _ in
            guard let self, let http = urlResponse as? HTTPURLResponse else {
                DispatchQueue.main.async { self?.store.markStale() }
                return
            }

            // A 401 means the token is no longer valid (expired / revoked).
            if http.statusCode == 401 {
                DispatchQueue.main.async {
                    self.store.markStale()
                    self.authManager.refreshAvailability()
                }
                return
            }

            // Usage headers ride along on both 200 and 429 (rate-limited) responses.
            guard let snapshot = UsageSnapshot(headers: http.allHeaderFields) else {
                DispatchQueue.main.async { self.store.markStale() }
                return
            }
            DispatchQueue.main.async { self.store.update(from: snapshot) }
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
