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

    init() {
        refreshAvailability()
    }

    /// Current OAuth bearer token, or nil if unavailable/expired.
    var accessToken: String? {
        guard let creds = ClaudeCodeKeychain.loadToken() else { return nil }
        if let expiry = creds.expiresAt, expiry < Date() { return nil }   // expired
        return creds.token
    }

    /// Re-evaluates whether a usable credential is present. Call after a 401
    /// or when the app becomes active, since the token can be refreshed or
    /// revoked by Claude Code out-of-band.
    func refreshAvailability() {
        if let creds = ClaudeCodeKeychain.loadToken() {
            let valid = creds.expiresAt.map { $0 >= Date() } ?? true
            isAuthenticated = valid
            subscriptionPlan = creds.plan
        } else {
            isAuthenticated = false
            subscriptionPlan = nil
        }
    }
}
