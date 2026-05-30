// UsageStore.swift — Observable usage data store
// UsagePoller.swift — 60-second polling of the Anthropic usage endpoint

import Foundation

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
        sessionPercent = l.sessionUsagePercent
        weeklyPercent  = l.weeklyUsagePercent
        sessionResetAt = l.sessionResetAt
        weeklyResetAt  = l.weeklyResetAt
        lastUpdated    = Date()
        isStale        = false
        writeToAppGroup()
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
    private let interval: TimeInterval = 60

    init(store: UsageStore, authManager: AuthManager) {
        self.store       = store
        self.authManager = authManager
    }

    func start() {
        poll()
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.poll()
        }
    }

    func stop() { timer?.invalidate(); timer = nil }

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
