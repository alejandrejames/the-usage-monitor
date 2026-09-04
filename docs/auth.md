# Authentication

> **History:** The original design used a WebView login that captured the
> `claude.ai` `sessionKey` cookie. That never worked: `api.claude.ai` does not
> resolve, and `claude.ai/api/oauth/usage` is behind a Cloudflare managed bot
> challenge that a plain HTTP client (with only `sessionKey`) cannot pass — it
> returns a 403 "Just a moment…" page. The app reuses Claude Code's OAuth token
> instead.

## How it works

There is **no in-app login**. The app reuses the OAuth credential the `claude`
CLI (Claude Code) already stores on the machine, and never writes one of its
own. The user manages auth entirely through the CLI; there is no `logout()`.

Implementation: [`crates/core/src/credentials.rs`](../crates/core/src/credentials.rs).

## Where the credential lives

| Platform | Location |
|---|---|
| macOS | login keychain, generic-password, service `Claude Code-credentials` |
| Linux | `$CLAUDE_CONFIG_DIR/.credentials.json` else `~/.claude/.credentials.json` |
| Windows | `%CLAUDE_CONFIG_DIR%\.credentials.json` else `%USERPROFILE%\.claude\.credentials.json` |

The value is JSON. The app reads exactly one key out of it:

```json
{
  "claudeAiOauth": {
    "accessToken":  "sk-ant-oat01-…",   // Bearer token
    "refreshToken": "sk-ant-ort01-…",
    "expiresAt":    1788512886774,      // epoch MILLISECONDS
    "subscriptionType": "pro"
  }
}
```

**The blob holds more than this app's token.** On macOS it also carries OAuth
tokens — including client secrets and refresh tokens — for every MCP server the
user has authorised. Nothing in the codebase logs the blob, a token, or any
substring of one; the `probe` binary prints a character count and nothing more.

Newer Claude Code versions add fields (`rateLimitTier`,
`refreshTokenExpiresAt`) that the decoder ignores rather than rejects.

## The macOS source chain

macOS tries three sources in order, and logs which one won:

1. `~/.claude/.credentials.json` — usually absent, but Claude Code writes it
   when a keychain write is rejected (a locked keychain over SSH, say).
2. `/usr/bin/security find-generic-password -s "Claude Code-credentials" -w`
3. The native Security framework (`ItemSearchOptions`, service-only query).

**The order is measured, not assumed.** The keychain item's `partition_id` list
contains only `apple-tool:`, which `/usr/bin/security` satisfies and a
third-party binary does not. Measured against the live item:

| Run | Native API | `security` CLI |
|---|---|---|
| First ever | **9.90 s** — ACL prompt shown | **31.8 ms** — no prompt |
| After a rebuild | **11.35 s** — prompt **again** | **34.2 ms** — no prompt |

The ~10 s timings are the ACL dialog waiting on a human. Clicking "Always
Allow" pins the grant to the binary's **cdhash**, so any code change revokes it
— and Claude Code rewrites the item on every token refresh, resetting the ACL
outright. The native path can therefore never be primary for an unsigned build.
Only a stable *designated requirement* (a real signing identity) survives
rebuilds.

See [cross-platform.md](cross-platform.md) for the full Spike A write-up.

## Caching and expiry

Every uncached read on macOS risks the ACL prompt, so the resolved credential
is held in memory between polls and only re-read when it is missing or expired.
A **10-second refetch floor** stops a burst of 401s from becoming a burst of
prompts.

A credential with no `expiresAt` is treated as non-expiring. On HTTP 401 the
cache is invalidated so the next poll re-reads — Claude Code may have rotated
the token out of band. Running any `claude` command refreshes the stored token;
the app picks it up on the next poll.

## Verifying

```bash
make probe
```

Prints which source resolved, the plan, and the expiry — never token material.
