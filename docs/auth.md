# Authentication

## Method: WebView session cookie

ClaudeUsage uses **only** the WebView login method. No OAuth token paste, no `claude setup-token`, no API keys.

This is the correct approach for a claude.ai subscription user because:
- It reads your own session the same way the browser does.
- It requires no developer credentials beyond a claude.ai account.
- It is not subject to the February 2026 Anthropic policy that restricts OAuth tokens in third-party tools.

---

## Login flow (step by step)

```
User opens app (first launch)
    │
    ▼
AuthManager.init() checks Keychain
    │
    ├─ sessionKey found → isAuthenticated = true → skip login
    │
    └─ not found → show LoginPromptView in popover
                        │
                        ▼
                  User taps "Sign in with Claude…"
                        │
                        ▼
                  LoginView presented as sheet
                        │
                        ▼
                  WKWebView loads https://claude.ai/login
                        │
                        ▼
                  AuthManager attached as WKHTTPCookieStoreObserver
                        │
                        ▼
                  User logs in (Google / email / SSO)
                        │
                        ▼
                  cookiesDidChange fires
                  AuthManager finds cookie where name == "sessionKey"
                        │
                        ▼
                  KeychainService.save(sessionCookie.value)
                  isAuthenticated = true
                  Observer removed from cookie store
                        │
                        ▼
                  LoginView auto-dismisses (onChange of isAuthenticated)
                        │
                        ▼
                  PopoverView shows live usage data
```

---

## WKWebView configuration

```swift
let config  = WKWebViewConfiguration()
let webView = WKWebView(frame: .zero, configuration: config)
webView.customUserAgent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/537.36 Safari/537.36"
```

The custom User-Agent presents the WebView as a desktop Safari browser so claude.ai renders its full login page rather than a stripped mobile view.

---

## Cookie detection

```swift
func cookiesDidChange(in store: WKHTTPCookieStore) {
    store.getAllCookies { cookies in
        if let sessionCookie = cookies.first(where: { $0.name == "sessionKey" }) {
            KeychainService.save(sessionCookie.value)
            self.isAuthenticated = true
            self.webView?.configuration.websiteDataStore
                .httpCookieStore.remove(self)   // unsubscribe once captured
        }
    }
}
```

The observer is removed immediately after capture to avoid unnecessary callbacks.

---

## Keychain storage

| Attribute | Value |
|---|---|
| `kSecClass` | `kSecClassGenericPassword` |
| `kSecAttrService` | `com.you.claudeusage` |
| `kSecAttrAccount` | `claude_session_key` |
| `kSecAttrAccessible` | `kSecAttrAccessibleWhenUnlocked` |

`kSecAttrAccessibleWhenUnlocked` means the item is accessible whenever the device is unlocked. It survives app restarts and system reboots. It is not synced to iCloud Keychain.

`KeychainService.save()` always calls `SecItemDelete` before `SecItemAdd` to replace any stale entry cleanly.

---

## Logout

`AuthManager.logout()` calls `KeychainService.delete()` and sets `isAuthenticated = false`. This immediately shows the login prompt in the popover. The WebView session (cookies in the WKWebView data store) is **not** cleared — the user remains logged in to claude.ai on the web.

---

## Session expiry

The `sessionKey` cookie does not have a fixed TTL documented publicly. If polling returns an auth error (HTTP 401 or 403), `UsageStore.markStale()` is called and the menu bar icon fades. The user must sign out and log in again via the Settings panel to refresh the session.

No automatic re-authentication is implemented. This is intentional — silent re-auth would require storing credentials beyond the session key.

---

## iOS notes

`LoginView.swift` contains both `NSViewRepresentable` (macOS) and `UIViewRepresentable` (iOS) implementations gated with `#if os(macOS)`. Both load the same URL and use the same `AuthManager`. The `AuthManager` itself is platform-agnostic — it uses `WebKit` which is available on both platforms.
