# Data flow

## Poll cycle (macOS)

```
App launch
    │
    ▼
UsagePoller.start()        (kicked off in ClaudeUsageApp.init when authenticated)
    │
    ├─── immediate poll()
    │
    └─── Timer fires every N seconds (default 60, user-configurable)
              │
              ▼
         AuthManager.accessToken   (reads Claude Code's keychain item;
              │                      nil if missing or expiresAt passed)
              ├─ nil → skip
              │
              └─ token → build URLRequest
                              │
                              ▼
                         POST https://api.anthropic.com/v1/messages   (1-token body)
                         Authorization: Bearer <accessToken>
                         anthropic-beta: oauth-2025-04-20
                         anthropic-version: 2023-06-01
                              │
                    ┌─────────┴──────────┐
                 200 / 429 + headers   401 / network error
                    │                       │
                    ▼                       ▼
              UsageSnapshot(headers:)  UsageStore.markStale()
                    │                 (401 → AuthManager.refreshAvailability())
                    ▼                 menu bar icon → 50% opacity
              UsageStore.update(from:)
                    ├─ sessionPercent ← 5h-utilization × 100
                    ├─ weeklyPercent  ← 7d-utilization × 100
                    ├─ sessionResetAt ← 5h-reset (epoch)
                    ├─ weeklyResetAt  ← 7d-reset (epoch)
                    ├─ lastUpdated = Date()
                    ├─ isStale = false
                    └─ writeToAppGroup()
                              │
                              ▼
                         UserDefaults(suiteName: "group.com.you.claudeusage")
                         sessionPercent, weeklyPercent,
                         sessionResetAt, weeklyResetAt, lastUpdated
                              │
                ┌─────────────┴─────────────┐
                ▼                           ▼
         WidgetKit reads               iOS app reads
         (TimelineProvider)            (UsageStore.loadFromAppGroup)
```

Only the **macOS** app fetches from the network. The widget and the iOS app are
read-only viewers of the App Group container.

## Request & headers

```
POST https://api.anthropic.com/v1/messages
Authorization: Bearer <accessToken>
anthropic-beta: oauth-2025-04-20
anthropic-version: 2023-06-01
content-type: application/json

{ "model": "claude-haiku-4-5-20251001", "max_tokens": 1,
  "messages": [{ "role": "user", "content": "." }] }
```

The completion is discarded — we only want the rate-limit **response headers**,
which ride along on both `200` and `429` responses:

| Header | Meaning |
|---|---|
| `anthropic-ratelimit-unified-5h-utilization` | session usage, **fraction 0–1** |
| `anthropic-ratelimit-unified-7d-utilization` | weekly usage, **fraction 0–1** |
| `anthropic-ratelimit-unified-5h-reset` | session reset, **epoch seconds** |
| `anthropic-ratelimit-unified-7d-reset` | weekly reset, **epoch seconds** |

`UsageSnapshot(headers:)` parses these; utilization is `× 100` for a percent.

> Earlier this used `GET /api/oauth/usage` (a different, undocumented endpoint
> returning `five_hour`/`seven_day` JSON). The headers approach is what mature
> usage monitors use and is more durable. Both require reusing Claude Code's
> OAuth token, which is against Anthropic's Consumer ToS (see README).

## App Group keys

The macOS app writes these display-safe (non-secret) values; the widget and iOS
app read them. The OAuth token is **never** written here.

| Key | Type | Meaning |
|---|---|---|
| `sessionPercent` | Double | five-hour utilization |
| `weeklyPercent` | Double | seven-day utilization |
| `sessionResetAt` | Double (epoch) | five-hour reset time |
| `weeklyResetAt` | Double (epoch) | seven-day reset time |
| `lastUpdated` | Double (epoch) | last successful poll |
