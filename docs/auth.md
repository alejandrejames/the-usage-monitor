# Authentication

> **History:** The original design used a WebView login that captured the
> `claude.ai` `sessionKey` cookie. That never worked: `api.claude.ai` does not
> resolve, and `claude.ai/api/oauth/usage` is behind a Cloudflare managed bot
> challenge that a plain `URLSession` (with only `sessionKey`) cannot pass —
> it returns a 403 "Just a moment…" page. The app now reuses Claude Code's
> OAuth token instead. See `real-usage-api-contract` in project memory.

## How it works now

There is **no in-app login**. The macOS app reuses the OAuth credential that
the `claude` CLI (Claude Code) already stores on the machine.

### macOS

`AuthManager` reads a generic-password item from the **login keychain**:

| Attribute | Value |
|---|---|
| `kSecClass` | `kSecClassGenericPassword` |
| `kSecAttrService` | `Claude Code-credentials` |

The item's value is JSON. The relevant part:

```json
{
  "claudeAiOauth": {
    "accessToken":  "sk-ant-oat01-…",   // Bearer token for the usage API
    "refreshToken": "sk-ant-ort01-…",
    "expiresAt":    1780135050513,        // epoch milliseconds
    "subscriptionType": "pro"
  }
}
```

- `AuthManager.accessToken` returns `claudeAiOauth.accessToken`, or `nil` if the
  item is missing or `expiresAt` has passed.
- `AuthManager.isAuthenticated` is true when a non-expired token is present.
- `AuthManager.refreshAvailability()` re-reads the keychain (call after a 401 or
  when the app becomes active). Claude Code refreshes the token out-of-band; the
  app simply re-reads it.
- There is **no `logout()`**. The user manages auth with the `claude` CLI.

**The macOS app must not be sandboxed.** A sandboxed app cannot read a
login-keychain item it did not create. `ClaudeUsage.entitlements` therefore
omits `com.apple.security.app-sandbox`. This rules out App Store distribution,
which was never a goal.

### iOS

iOS cannot read the Mac's keychain and has no token of its own. The iOS
`AuthManager` branch reports `isAuthenticated` based on whether the App Group
has ever received synced data, and `UsageStore.loadFromAppGroup()` reads the
percentages the macOS app wrote. iOS never calls the network.

## Token expiry

If a poll returns HTTP 401, the token is expired/revoked. `UsageStore.markStale()`
fades the menu-bar icon and `AuthManager.refreshAvailability()` re-checks the
keychain. Running any `claude` command refreshes the stored token; the app picks
up the new value on its next poll or re-check.
