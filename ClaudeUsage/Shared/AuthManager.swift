// AuthManager.swift — WebView login + Keychain credential store
// Captures the `sessionKey` cookie from claude.ai and saves it securely.

import WebKit
import Security

// MARK: - Keychain helper

enum KeychainService {
    private static let account = "claude_session_key"
    private static let service = "com.you.claudeusage"

    static func save(_ value: String) {
        guard let data = value.data(using: .utf8) else { return }
        let query: [CFString: Any] = [
            kSecClass:            kSecClassGenericPassword,
            kSecAttrService:      service,
            kSecAttrAccount:      account,
            kSecValueData:        data,
            kSecAttrAccessible:   kSecAttrAccessibleWhenUnlocked
        ]
        SecItemDelete(query as CFDictionary)          // remove stale entry first
        SecItemAdd(query as CFDictionary, nil)
    }

    static func load() -> String? {
        let query: [CFString: Any] = [
            kSecClass:            kSecClassGenericPassword,
            kSecAttrService:      service,
            kSecAttrAccount:      account,
            kSecReturnData:       true,
            kSecMatchLimit:       kSecMatchLimitOne
        ]
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              let string = String(data: data, encoding: .utf8) else { return nil }
        return string
    }

    static func delete() {
        let query: [CFString: Any] = [
            kSecClass:       kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account
        ]
        SecItemDelete(query as CFDictionary)
    }
}

// MARK: - AuthManager

@Observable
final class AuthManager: NSObject, WKHTTPCookieStoreObserver {

    var isAuthenticated: Bool = false
    var sessionKey: String? { KeychainService.load() }

    private var webView: WKWebView?

    override init() {
        super.init()
        isAuthenticated = KeychainService.load() != nil
    }

    // Called by LoginView to attach the observer
    func attachCookieObserver(to webView: WKWebView) {
        self.webView = webView
        webView.configuration.websiteDataStore.httpCookieStore.add(self)
    }

    // WKHTTPCookieStoreObserver — fires on every cookie change
    func cookiesDidChange(in store: WKHTTPCookieStore) {
        store.getAllCookies { [weak self] cookies in
            guard let self else { return }
            if let sessionCookie = cookies.first(where: { $0.name == "sessionKey" }) {
                DispatchQueue.main.async {
                    KeychainService.save(sessionCookie.value)
                    self.isAuthenticated = true
                    // Remove observer — we have what we need
                    self.webView?.configuration.websiteDataStore
                        .httpCookieStore.remove(self)
                }
            }
        }
    }

    func logout() {
        KeychainService.delete()
        isAuthenticated = false
    }
}
