// StatusStore.swift — Claude service status from status.claude.com

import Foundation
import Network

// MARK: - Service status

/// Health of one Claude service, mapped from Statuspage's component status.
enum ServiceHealth: String {
    case operational
    case degraded
    case partialOutage
    case majorOutage
    case maintenance
    case unknown

    /// Statuspage component `status` strings → our cases.
    init(apiValue: String) {
        switch apiValue {
        case "operational":         self = .operational
        case "degraded_performance": self = .degraded
        case "partial_outage":      self = .partialOutage
        case "major_outage":        self = .majorOutage
        case "under_maintenance":   self = .maintenance
        default:                    self = .unknown
        }
    }

    var label: String {
        switch self {
        case .operational:   return "Operational"
        case .degraded:      return "Degraded"
        case .partialOutage: return "Partial outage"
        case .majorOutage:   return "Major outage"
        case .maintenance:   return "Maintenance"
        case .unknown:       return "Unknown"
        }
    }

    /// True for anything the user would want to notice at a glance.
    var isHealthy: Bool { self == .operational }
}

/// One row in the status accordion.
struct ServiceStatus: Identifiable, Equatable {
    let id:     String          // Statuspage component id
    let name:   String          // display name
    let health: ServiceHealth

    static func == (a: ServiceStatus, b: ServiceStatus) -> Bool {
        a.id == b.id && a.name == b.name && a.health == b.health
    }
}

// MARK: - StatusStore

/// Polls status.claude.com's public Statuspage API for the two components we
/// surface: claude.ai and Claude Code. The endpoint needs no auth — it is the
/// same JSON the public status page renders from.
@Observable
final class StatusStore {

    /// Components we display, in the order they appear in the popover. Matched
    /// by Statuspage component id (stable) with a name fallback, so a cosmetic
    /// rename upstream doesn't blank the row.
    private static let tracked: [(id: String, name: String)] = [
        (id: "rwppv331jlwc", name: "claude.ai"),
        (id: "yyzkbfz2thpt", name: "Claude Code"),
    ]

    var services:    [ServiceStatus] = []
    var lastUpdated: Date?           = nil
    var isStale:     Bool            = false   // last fetch failed

    /// Worst health across the tracked services — drives the collapsed summary.
    var overall: ServiceHealth {
        if services.contains(where: { $0.health == .majorOutage })   { return .majorOutage }
        if services.contains(where: { $0.health == .partialOutage }) { return .partialOutage }
        if services.contains(where: { $0.health == .degraded })      { return .degraded }
        if services.contains(where: { $0.health == .maintenance })   { return .maintenance }
        if services.isEmpty                                          { return .unknown }
        return .operational
    }

    /// True when every tracked service is operational.
    var allOperational: Bool { !services.isEmpty && overall == .operational }

    fileprivate func apply(_ components: [(id: String, name: String, status: String)]) {
        services = Self.tracked.map { wanted in
            let match = components.first { $0.id == wanted.id }
                     ?? components.first { $0.name == wanted.name }
            return ServiceStatus(
                id:     wanted.id,
                name:   match?.name ?? wanted.name,
                health: match.map { ServiceHealth(apiValue: $0.status) } ?? .unknown
            )
        }
        lastUpdated = Date()
        isStale     = false
    }

    fileprivate func markStale() { isStale = true }
}

// MARK: - StatusPoller

/// Fetches the status summary on a slow cadence. Status changes rarely, and the
/// endpoint is unauthenticated, so this is deliberately independent of the usage
/// poller — a failing status fetch must never affect the usage reading.
final class StatusPoller {

    private let store: StatusStore
    private var timer: Timer?

    private let endpoint = URL(string: "https://status.claude.com/api/v2/summary.json")!

    /// Status moves far more slowly than usage; 5 minutes is plenty and keeps us
    /// well clear of any rate limiting on the public status API.
    private let interval: TimeInterval = 300

    private let pathMonitor = NWPathMonitor()
    private let pathQueue   = DispatchQueue(label: "com.you.claudeusage.status-network")
    private var wasOnline   = true

    init(store: StatusStore) {
        self.store = store
    }

    deinit { pathMonitor.cancel() }

    func start() {
        fetch()
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            self?.fetch()
        }
        observeReachability()
    }

    func stop() { timer?.invalidate(); timer = nil }

    /// Immediate refresh, re-aligning the timer (used by the popover's Refresh).
    func refreshNow() {
        fetch()
        if timer != nil { start() }
    }

    // Re-fetch as soon as connectivity returns, so the panel isn't stuck showing
    // a stale outage state after the network comes back.
    private func observeReachability() {
        pathMonitor.pathUpdateHandler = { [weak self] path in
            guard let self else { return }
            let online = path.status == .satisfied
            defer { self.wasOnline = online }
            if online, !self.wasOnline {
                DispatchQueue.main.async { [weak self] in self?.fetch() }
            }
        }
        pathMonitor.start(queue: pathQueue)
    }

    private func fetch() {
        var request = URLRequest(url: endpoint)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        // Statuspage is CDN-cached; skip the local cache so we see fresh state.
        request.cachePolicy = .reloadIgnoringLocalCacheData

        URLSession.shared.dataTask(with: request) { [weak self] data, urlResponse, _ in
            guard let self else { return }
            guard let data,
                  let http = urlResponse as? HTTPURLResponse,
                  (200..<300).contains(http.statusCode),
                  let parsed = Self.parseComponents(data) else {
                DispatchQueue.main.async { self.store.markStale() }
                return
            }
            DispatchQueue.main.async { self.store.apply(parsed) }
        }.resume()
    }

    /// Pulls (id, name, status) out of the summary payload. Parsed loosely with
    /// JSONSerialization so unrelated parts of the feed (incident bodies, new
    /// fields) can never fail the decode of the few keys we actually read.
    private static func parseComponents(_ data: Data) -> [(id: String, name: String, status: String)]? {
        guard let root = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any],
              let components = root["components"] as? [[String: Any]] else { return nil }
        return components.compactMap { c in
            guard let id = c["id"] as? String,
                  let name = c["name"] as? String,
                  let status = c["status"] as? String else { return nil }
            return (id: id, name: name, status: status)
        }
    }
}
