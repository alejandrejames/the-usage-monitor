// AuthManager.swift — OAuth credential access
// macOS: reads Claude Code's existing OAuth token from the login keychain.
// iOS:   has no local token; it views data synced from the Mac via App Group.

import Foundation
import Security

// MARK: - Claude Code credential model

/// The subset of Claude Code's keychain JSON we care about.
private struct ClaudeCodeCredentials: Decodable {
    struct OAuth: Decodable {
        let accessToken:  String
        let refreshToken: String?
        let expiresAt:    Double?       // epoch milliseconds
        let subscriptionType: String?
    }
    let claudeAiOauth: OAuth
}

// MARK: - Keychain reader (macOS)

#if os(macOS)
/// Reads the generic-password item the `claude` CLI writes to the login
/// keychain (service "Claude Code-credentials"). Requires the macOS app to be
/// un-sandboxed so it can reach a login-keychain item it did not create.
enum ClaudeCodeKeychain {
    private static let service = "Claude Code-credentials"

    static func loadToken() -> (token: String, expiresAt: Date?, plan: String?)? {
        let query: [CFString: Any] = [
            kSecClass:       kSecClassGenericPassword,
            kSecAttrService: service,
            kSecReturnData:  true,
            kSecMatchLimit:  kSecMatchLimitOne,
        ]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              let creds = try? JSONDecoder().decode(ClaudeCodeCredentials.self, from: data)
        else { return nil }

        let oauth = creds.claudeAiOauth
        let expiry = oauth.expiresAt.map { Date(timeIntervalSince1970: $0 / 1000) }
        return (oauth.accessToken, expiry, oauth.subscriptionType)
    }
}
#endif

// MARK: - AuthManager

@Observable
final class AuthManager {

    var isAuthenticated: Bool = false
    var subscriptionPlan: String? = nil

    // In-memory cache of the last good token. Each keychain read can trigger the
    // macOS ACL prompt (the item is owned by the `claude` CLI), so we read once
    // and reuse the cached value for routine polls — including after a network
    // reconnect. We only hit the keychain again when the cache is empty, the
    // token has expired, or a 401 forces a re-check (see invalidateToken()).
    private var cachedToken:  String? = nil
    private var cachedExpiry: Date?   = nil

    init() {
        refreshAvailability()
    }

    /// Current OAuth bearer token, or nil if unavailable/expired.
    /// Reuses the in-memory cache when still valid to avoid a keychain prompt.
    var accessToken: String? {
        if let token = cachedToken, let expiry = cachedExpiry, expiry >= Date() {
            return token                       // cache hit — no keychain access
        }
        if let token = cachedToken, cachedExpiry == nil {
            return token                       // cached, no expiry metadata
        }
        return reloadToken()                   // cache miss/expired — read keychain
    }

    /// Drops the in-memory token so the next `accessToken` re-reads the keychain.
    /// Call after a 401: Claude Code may have refreshed the token out-of-band.
    func invalidateToken() {
        cachedToken  = nil
        cachedExpiry = nil
    }

    /// Re-evaluates whether a usable credential is present. Call after a 401
    /// or when the app becomes active, since the token can be refreshed or
    /// revoked by Claude Code out-of-band.
    func refreshAvailability() {
        invalidateToken()
        _ = reloadToken()
    }

    // MARK: - Keychain read + cache fill

    @discardableResult
    private func reloadToken() -> String? {
        guard let creds = ClaudeCodeKeychain.loadToken() else {
            cachedToken = nil; cachedExpiry = nil
            isAuthenticated = false
            subscriptionPlan = nil
            return nil
        }
        let valid = creds.expiresAt.map { $0 >= Date() } ?? true
        subscriptionPlan = creds.plan
        isAuthenticated  = valid
        if valid {
            cachedToken  = creds.token
            cachedExpiry = creds.expiresAt
            return creds.token
        } else {
            cachedToken = nil; cachedExpiry = nil
            return nil
        }
    }
}
