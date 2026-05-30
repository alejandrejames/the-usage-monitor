# Data flow

## Poll cycle

```
App launch
    │
    ▼
UsagePoller.start()
    │
    ├─── immediate poll()
    │
    └─── Timer fires every N seconds (default 60, user-configurable)
              │
              ▼
         AuthManager.sessionKey   read from Keychain
              │
              ├─ nil → skip (not logged in)
              │
              └─ value → build URLRequest
                              │
                              ▼
                         GET https://api.claude.ai/api/oauth/usage
                         Cookie: sessionKey=<value>
                         User-Agent: claude-code/1.0.0
                         Accept: application/json
                              │
                    ┌─────────┴──────────┐
                 success              failure / 401 / network error
                    │                       │
                    ▼                       ▼
              decode UsageResponse    UsageStore.markStale()
                    │                 menu bar icon → 50% opacity
                    ▼
              UsageStore.update(from:)
                    ├─ sessionPercent
                    ├─ weeklyPercent
                    ├─ sessionResetAt
                    ├─ weeklyResetAt
                    ├─ lastUpdated = Date()
                    ├─ isStale = false
                    └─ writeToAppGroup()
                              │
                              ▼
                         UserDefaults(suiteName: "group.com.you.claudeusage")
                         sessionPercent, weeklyPercent,
                         sessionResetAt, weeklyResetAt, lastUpdated
```

---

## API endpoint

```
GET https://api.claude.ai/api/oauth/usage
```

This is Anthropic's internal endpoint used by Claude Code's `/usage` command. It is not publicly documented.

**Required headers:**

| Header | Value | Why |
|---|---|---|
| `Cookie` | `sessionKey=<value>` | Authentication |
| `User-Agent` | `claude-code/1.0.0` | Without this header you hit an aggressively rate-limited bucket and receive persistent 429s |
| `Accept` | `application/json` | Response format |

**Expected response shape (approximate):**

```json
{
  "limits": {
    "sessionUsagePercent": 72.0,
    "weeklyUsagePercent": 41.0,
    "sessionResetAt": "2026-05-30T15:45:00Z",
    "weeklyResetAt": "2026-06-02T00:00:00Z"
  }
}
```

The exact JSON field names may differ from what is shown here. If the response shape differs, update `UsageResponse` in `UsageStore.swift` to match. Use `JSONDecoder` with `.dateDecodingStrategy = .iso8601`.

---

## UsageStore model

```swift
@Observable final class UsageStore {
    var sessionPercent:  Double = 0
    var weeklyPercent:   Double = 0
    var sessionResetAt:  Date?
    var weeklyResetAt:   Date?
    var lastUpdated:     Date?
    var isStale:         Bool = false
}
```

All views observe these properties directly — no intermediate ViewModels.

---

## App Group writes

`UsageStore.writeToAppGroup()` is called on every successful poll:

```swift
private func writeToAppGroup() {
    guard let defaults = UserDefaults(suiteName: "group.com.you.claudeusage") else { return }
    defaults.set(sessionPercent,                         forKey: "sessionPercent")
    defaults.set(weeklyPercent,                          forKey: "weeklyPercent")
    defaults.set(sessionResetAt?.timeIntervalSince1970,  forKey: "sessionResetAt")
    defaults.set(weeklyResetAt?.timeIntervalSince1970,   forKey: "weeklyResetAt")
    defaults.set(Date().timeIntervalSince1970,           forKey: "lastUpdated")
}
```

Timestamps are stored as `Double` (Unix epoch seconds) because `Date` is not directly `UserDefaults`-compatible in a cross-process safe way.

The `sessionKey` secret is **never** written to `UserDefaults` — it stays in Keychain only.

---

## Stale detection

`UsageStore.isStale` is set to `true` when a poll fails (network error, auth error, or bad response).

The menu bar label reacts:
```swift
HStack { … }
    .opacity(store.isStale ? 0.5 : 1)
```

The widget considers data stale if `lastUpdated` is more than 5 minutes ago:
```swift
let isStale = updated.map { Date().timeIntervalSince($0) > 300 } ?? true
```

---

## Polling interval

Default: 60 seconds. Configurable via `@AppStorage("refreshInterval")` in `SettingsView`:

| Option | Value |
|---|---|
| 30 s | 30 |
| 60 s (default) | 60 |
| 2 min | 120 |
| 5 min | 300 |

`UsagePoller` reads the current interval at `start()` time. To make interval changes take effect without restarting the app, `UsagePoller` needs a `restart()` method that calls `stop()` then `start()`. Add this if needed.

---

## Error handling

| Error condition | Response |
|---|---|
| `sessionKey` nil (not logged in) | Skip poll silently |
| Network unreachable | `markStale()` |
| HTTP 401 / 403 | `markStale()` — user must re-login |
| HTTP 429 (rate limited) | `markStale()` — check `User-Agent` header |
| JSON decode failure | `markStale()` — may need to update `UsageResponse` model |
| App Group `UserDefaults` unavailable | Silent — widget shows last cached values |
