//! Reading Claude Code's OAuth credential, per platform.
//!
//! This is the only module in `core` with per-OS branches. Everything else is
//! pure logic; here the storage location genuinely differs:
//!
//! | Platform | Location |
//! |---|---|
//! | macOS   | login keychain, generic-password service `Claude Code-credentials` |
//! | Linux   | `$CLAUDE_CONFIG_DIR/.credentials.json` else `~/.claude/.credentials.json` |
//! | Windows | `%CLAUDE_CONFIG_DIR%\.credentials.json` else `%USERPROFILE%\.claude\.credentials.json` |
//!
//! **The credential blob is more sensitive than just this app's token.** On
//! macOS it also carries OAuth tokens for every MCP server the user has
//! authorised. Nothing here logs the blob, a token, or any substring of one —
//! only which source succeeded and whether a token was found.
//!
//! See `docs/cross-platform.md` for the Spike A measurements behind the macOS
//! source ordering.

use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

// MARK: - Credential model

/// The `claudeAiOauth` object, which is the only part of the credential file
/// this app reads. Sibling keys (`mcpOAuth`, `organizationUuid`) are ignored.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub access_token: String,
    /// Epoch **milliseconds**. Absent in some Claude Code versions, in which
    /// case the token is treated as non-expiring (matching `AuthManager.swift`).
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// e.g. "pro", "max". Shown in the popover.
    #[serde(default)]
    pub subscription_type: Option<String>,
}

/// Wrapper matching the on-disk / in-keychain JSON shape.
#[derive(Debug, Deserialize)]
struct CredentialFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Credentials,
}

impl Credentials {
    /// Parses the credential JSON, ignoring every key except `claudeAiOauth`.
    pub fn from_json(json: &str) -> Result<Self, CredentialError> {
        let parsed: CredentialFile =
            serde_json::from_str(json).map_err(|_| CredentialError::Malformed)?;
        if parsed.claude_ai_oauth.access_token.is_empty() {
            return Err(CredentialError::Malformed);
        }
        Ok(parsed.claude_ai_oauth)
    }

    /// True when the token has an expiry that has already passed. A credential
    /// with no expiry is treated as valid, matching `AuthManager.swift`'s
    /// `creds.expiresAt.map { $0 >= Date() } ?? true`.
    pub fn is_expired(&self) -> bool {
        self.is_expired_at(now_millis())
    }

    /// Testable form of [`is_expired`].
    pub fn is_expired_at(&self, now_ms: i64) -> bool {
        match self.expires_at {
            Some(expiry) => expiry < now_ms,
            None => false,
        }
    }
}

/// Current wall clock as epoch millis. Saturates rather than panicking if the
/// system clock is set before 1970.
fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// MARK: - Errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// No credential found by any source. The UI shows the "Claude Code not
    /// detected" prompt for this.
    NotFound,
    /// Found, but the JSON did not contain a usable `claudeAiOauth.accessToken`.
    Malformed,
    /// A source failed for an environment reason (permissions, missing HOME).
    /// The message never contains credential material.
    Unavailable(String),
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "no Claude Code credential found"),
            Self::Malformed => write!(f, "credential found but not readable"),
            Self::Unavailable(why) => write!(f, "credential source unavailable: {why}"),
        }
    }
}

impl std::error::Error for CredentialError {}

// MARK: - Source trait

/// Where a credential came from. Logged for diagnostics — the plan calls for
/// recording which source won, since the macOS ordering is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A plaintext `.credentials.json`.
    File,
    /// Shelling out to `/usr/bin/security` (macOS).
    SecurityCli,
    /// Native keychain API (macOS).
    Keychain,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "credentials file",
            Self::SecurityCli => "security CLI",
            Self::Keychain => "keychain API",
        }
    }
}

/// One place a credential might live.
///
/// `Send + Sync` because the host polls from a background thread, so the whole
/// source chain has to cross thread boundaries inside the shared app state.
pub trait CredentialSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    /// Returns the raw credential JSON, or an error if this source has nothing.
    fn load_json(&self) -> Result<String, CredentialError>;
}

/// A credential plus the source that produced it.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub credentials: Credentials,
    pub source: SourceKind,
}

/// Tries each source in order, returning the first usable credential.
///
/// A source that is merely absent is skipped. A source that returns
/// *malformed* data is also skipped rather than aborting the chain, so one
/// corrupt file cannot mask a working source further down.
pub fn resolve(sources: &[Box<dyn CredentialSource>]) -> Result<Resolved, CredentialError> {
    let mut last_error = CredentialError::NotFound;

    for source in sources {
        match source.load_json() {
            Ok(json) => match Credentials::from_json(&json) {
                Ok(credentials) => return Ok(Resolved { credentials, source: source.kind() }),
                Err(e) => last_error = e,
            },
            Err(CredentialError::NotFound) => continue,
            Err(e) => last_error = e,
        }
    }

    Err(last_error)
}

/// The platform's source chain, in priority order.
pub fn default_sources() -> Vec<Box<dyn CredentialSource>> {
    #[cfg(target_os = "macos")]
    {
        // Spike A (docs/cross-platform.md): the CLI reads the item in <40 ms
        // with no ACL prompt, while the native API prompts on every new cdhash.
        // The file is checked first because when it exists Claude Code wrote it
        // in preference to the keychain.
        vec![
            Box::new(file::FileSource::default_path()),
            Box::new(macos::SecurityCliSource),
            Box::new(macos::KeychainSource),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![Box::new(file::FileSource::default_path())]
    }
}

// MARK: - File source (all platforms)

pub mod file {
    use super::{CredentialError, CredentialSource, SourceKind};
    use std::path::{Path, PathBuf};

    /// Reads a plaintext `.credentials.json`. This is the only source on Linux
    /// and Windows, and the first one tried on macOS.
    pub struct FileSource {
        path: Option<PathBuf>,
    }

    impl FileSource {
        pub fn new(path: impl Into<PathBuf>) -> Self {
            Self { path: Some(path.into()) }
        }

        /// `$CLAUDE_CONFIG_DIR/.credentials.json` when that variable is set,
        /// else `~/.claude/.credentials.json`.
        pub fn default_path() -> Self {
            Self { path: default_credentials_path() }
        }

        pub fn path(&self) -> Option<&Path> {
            self.path.as_deref()
        }
    }

    impl CredentialSource for FileSource {
        fn kind(&self) -> SourceKind {
            SourceKind::File
        }

        fn load_json(&self) -> Result<String, CredentialError> {
            let path = self.path.as_ref().ok_or(CredentialError::NotFound)?;
            match std::fs::read_to_string(path) {
                Ok(json) => Ok(json),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    Err(CredentialError::NotFound)
                }
                // Report the io error kind, never the path contents.
                Err(e) => Err(CredentialError::Unavailable(e.kind().to_string())),
            }
        }
    }

    /// Resolves the credential file location, honouring `CLAUDE_CONFIG_DIR`.
    ///
    /// Claude Code documents this override for Linux and Windows. It is also
    /// respected on macOS so a user who sets it gets consistent behaviour.
    pub fn default_credentials_path() -> Option<PathBuf> {
        const FILE: &str = ".credentials.json";

        if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
            if !dir.is_empty() {
                return Some(PathBuf::from(dir).join(FILE));
            }
        }
        Some(home_dir()?.join(".claude").join(FILE))
    }

    /// The user's home directory.
    ///
    /// Hand-rolled rather than pulling in a crate: `HOME` covers macOS and
    /// Linux, and `USERPROFILE` covers Windows, which is the whole matrix.
    fn home_dir() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(
                || {
                    // Fall back to the HOMEDRIVE+HOMEPATH pair.
                    let drive = std::env::var_os("HOMEDRIVE")?;
                    let path = std::env::var_os("HOMEPATH")?;
                    let mut joined = std::ffi::OsString::from(drive);
                    joined.push(path);
                    Some(PathBuf::from(joined))
                },
            )
        }
        #[cfg(not(windows))]
        {
            std::env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
        }
    }
}

// MARK: - macOS sources

#[cfg(target_os = "macos")]
pub mod macos {
    use super::{CredentialError, CredentialSource, SourceKind};

    /// The generic-password service the `claude` CLI writes to.
    pub const SERVICE: &str = "Claude Code-credentials";

    /// Reads the keychain item by shelling out to `/usr/bin/security`.
    ///
    /// This is the primary macOS source, and the reason is measured rather
    /// than assumed. The keychain item's `partition_id` list contains only
    /// `apple-tool:`, which `/usr/bin/security` satisfies and a third-party
    /// binary does not. Spike A: this path returned the credential in ~32 ms
    /// with no prompt across five runs, while the native API below showed the
    /// ACL dialog on first run *and* again after a rebuild, because an
    /// "Always Allow" grant for an unsigned binary is pinned to its cdhash.
    pub struct SecurityCliSource;

    impl CredentialSource for SecurityCliSource {
        fn kind(&self) -> SourceKind {
            SourceKind::SecurityCli
        }

        fn load_json(&self) -> Result<String, CredentialError> {
            let output = std::process::Command::new("/usr/bin/security")
                .args(["find-generic-password", "-s", SERVICE, "-w"])
                .output()
                .map_err(|e| CredentialError::Unavailable(e.kind().to_string()))?;

            if !output.status.success() {
                // Exit 44 is "item not found"; anything else is an environment
                // problem. stderr is deliberately not propagated — it can echo
                // the query back.
                return Err(match output.status.code() {
                    Some(44) => CredentialError::NotFound,
                    Some(code) => CredentialError::Unavailable(format!("security exited {code}")),
                    None => CredentialError::Unavailable("security terminated".into()),
                });
            }

            String::from_utf8(output.stdout)
                .map(|s| s.trim().to_string())
                .map_err(|_| CredentialError::Malformed)
        }
    }

    /// Reads the keychain item through the native Security framework.
    ///
    /// Last resort: this triggers the ACL prompt for any binary not already in
    /// the item's applications list, and the grant does not survive a rebuild
    /// unless the app has a stable designated requirement (i.e. a real signing
    /// identity). Kept so a properly signed build can skip the subprocess.
    ///
    /// Queries by service only, with no account, matching `AuthManager.swift`.
    pub struct KeychainSource;

    impl CredentialSource for KeychainSource {
        fn kind(&self) -> SourceKind {
            SourceKind::Keychain
        }

        fn load_json(&self) -> Result<String, CredentialError> {
            use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};

            let results = ItemSearchOptions::new()
                .class(ItemClass::generic_password())
                .service(SERVICE)
                .load_data(true)
                .limit(Limit::Max(1))
                .search()
                .map_err(|_| CredentialError::NotFound)?;

            let data = results
                .into_iter()
                .find_map(|r| match r {
                    SearchResult::Data(d) => Some(d),
                    _ => None,
                })
                .ok_or(CredentialError::NotFound)?;

            String::from_utf8(data).map_err(|_| CredentialError::Malformed)
        }
    }
}

// MARK: - Cached provider

/// Caches a resolved credential in memory between polls.
///
/// This is ported from `AuthManager.swift` and is load-bearing on macOS: every
/// keychain read can trigger the ACL prompt, so routine 60-second polling must
/// not hit the keychain. It also rate-limits re-reads so a burst of 401s cannot
/// become a burst of prompts.
pub struct CachedCredentials {
    sources: Vec<Box<dyn CredentialSource>>,
    cached: Option<Resolved>,
    /// When the last read was attempted, epoch millis.
    last_attempt_ms: Option<i64>,
    /// Minimum gap between reads forced by `invalidate()`.
    min_refetch_interval_ms: i64,
}

impl CachedCredentials {
    /// A 10-second floor is long enough that a 401 storm collapses into one
    /// read, and short enough that a genuine token rotation is picked up on the
    /// next poll.
    pub const DEFAULT_MIN_REFETCH_MS: i64 = 10_000;

    pub fn new(sources: Vec<Box<dyn CredentialSource>>) -> Self {
        Self {
            sources,
            cached: None,
            last_attempt_ms: None,
            min_refetch_interval_ms: Self::DEFAULT_MIN_REFETCH_MS,
        }
    }

    /// The platform default chain.
    pub fn with_default_sources() -> Self {
        Self::new(default_sources())
    }

    /// Current token, reading from the sources only when the cache is empty or
    /// the cached token has expired.
    pub fn token(&mut self) -> Result<&Resolved, CredentialError> {
        self.token_at(now_millis())
    }

    /// Testable form of [`token`].
    pub fn token_at(&mut self, now_ms: i64) -> Result<&Resolved, CredentialError> {
        let usable = self
            .cached
            .as_ref()
            .is_some_and(|resolved| !resolved.credentials.is_expired_at(now_ms));

        if !usable {
            // Rate-limit re-reads. The floor is keyed on the last *attempt*,
            // not on whether a value is cached: `invalidate()` clears the
            // cache, so a cache-presence check here would never fire and a
            // burst of 401s would become a burst of keychain prompts.
            //
            // The very first read is always allowed through.
            let within_floor = self
                .last_attempt_ms
                .is_some_and(|last| now_ms.saturating_sub(last) < self.min_refetch_interval_ms);

            if within_floor {
                // Inside the floor, serve whatever we last resolved. After an
                // `invalidate()` that is nothing, which is reported as such
                // rather than by hitting the source again.
                return self.cached.as_ref().ok_or(CredentialError::NotFound);
            }

            self.last_attempt_ms = Some(now_ms);
            self.cached = Some(resolve(&self.sources)?);
        }

        self.cached.as_ref().ok_or(CredentialError::NotFound)
    }

    /// Drops the cached token so the next [`token`] re-reads.
    ///
    /// Call after a 401: Claude Code may have rotated the credential out of
    /// band. The refetch floor still applies, so repeated 401s do not cause
    /// repeated reads.
    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    /// True when a usable, unexpired credential is currently cached.
    pub fn is_authenticated(&self) -> bool {
        self.is_authenticated_at(now_millis())
    }

    /// Testable form of [`is_authenticated`].
    pub fn is_authenticated_at(&self, now_ms: i64) -> bool {
        self.cached.as_ref().is_some_and(|r| !r.credentials.is_expired_at(now_ms))
    }

    /// Last known subscription plan, if any.
    pub fn plan(&self) -> Option<&str> {
        self.cached.as_ref()?.credentials.subscription_type.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const VALID: &str = r#"{
        "claudeAiOauth": {
            "accessToken": "sk-ant-oat01-example",
            "refreshToken": "sk-ant-ort01-example",
            "expiresAt": 1788512886774,
            "subscriptionType": "pro",
            "scopes": ["user:inference"]
        },
        "organizationUuid": "abc"
    }"#;

    // MARK: Parsing

    #[test]
    fn parses_the_claude_oauth_object() {
        let c = Credentials::from_json(VALID).expect("should parse");
        assert_eq!(c.access_token, "sk-ant-oat01-example");
        assert_eq!(c.expires_at, Some(1_788_512_886_774));
        assert_eq!(c.subscription_type.as_deref(), Some("pro"));
    }

    #[test]
    fn ignores_unrelated_sibling_keys() {
        // The real macOS item also stores MCP OAuth tokens for other servers.
        // Those must be ignored, not rejected.
        let json = r#"{
            "mcpOAuth": {
                "figma|abc": {"accessToken":"other","clientSecret":"s","expiresAt":1}
            },
            "claudeAiOauth": {"accessToken":"sk-ant-oat01-x"},
            "organizationUuid": "abc"
        }"#;
        let c = Credentials::from_json(json).expect("should parse");
        assert_eq!(c.access_token, "sk-ant-oat01-x");
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let json = r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-x"}}"#;
        let c = Credentials::from_json(json).expect("should parse");
        assert_eq!(c.expires_at, None);
        assert_eq!(c.subscription_type, None);
        // No expiry means non-expiring, matching AuthManager.swift.
        assert!(!c.is_expired_at(i64::MAX));
    }

    #[test]
    fn tolerates_unknown_new_fields() {
        // rateLimitTier and refreshTokenExpiresAt exist on the live item and
        // are not modelled; a future addition must not break the decode.
        let json = r#"{"claudeAiOauth":{
            "accessToken":"sk-ant-oat01-x",
            "rateLimitTier":"default_claude_ai",
            "refreshTokenExpiresAt":123,
            "somethingBrandNew":{"a":1}
        }}"#;
        assert!(Credentials::from_json(json).is_ok());
    }

    #[test]
    fn rejects_malformed_and_empty() {
        assert_eq!(Credentials::from_json("not json").unwrap_err(), CredentialError::Malformed);
        assert_eq!(Credentials::from_json("{}").unwrap_err(), CredentialError::Malformed);
        // Present but empty is not usable.
        assert_eq!(
            Credentials::from_json(r#"{"claudeAiOauth":{"accessToken":""}}"#).unwrap_err(),
            CredentialError::Malformed
        );
        // A file holding only MCP tokens is not a Claude credential.
        assert_eq!(
            Credentials::from_json(r#"{"mcpOAuth":{}}"#).unwrap_err(),
            CredentialError::Malformed
        );
    }

    // MARK: Expiry

    #[test]
    fn expiry_compares_in_milliseconds() {
        let c = Credentials::from_json(VALID).unwrap();
        let expiry = 1_788_512_886_774;
        assert!(!c.is_expired_at(expiry - 1));
        // Exactly at the boundary is still valid, matching Swift's `>=`.
        assert!(!c.is_expired_at(expiry));
        assert!(c.is_expired_at(expiry + 1));
    }

    // MARK: Chain resolution

    /// A source that returns a canned result and counts its calls. The counter
    /// is shared so a test can assert on it after the stub is boxed away.
    struct Stub {
        kind: SourceKind,
        result: Result<String, CredentialError>,
        calls: Arc<AtomicUsize>,
    }

    impl Stub {
        fn ok(kind: SourceKind, json: &str) -> Self {
            Self { kind, result: Ok(json.to_string()), calls: Arc::new(AtomicUsize::new(0)) }
        }
        fn err(kind: SourceKind, e: CredentialError) -> Self {
            Self { kind, result: Err(e), calls: Arc::new(AtomicUsize::new(0)) }
        }
        /// Handle to this stub's call counter.
        fn counter(&self) -> Arc<AtomicUsize> {
            Arc::clone(&self.calls)
        }
    }

    impl CredentialSource for Stub {
        fn kind(&self) -> SourceKind {
            self.kind
        }
        fn load_json(&self) -> Result<String, CredentialError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result.clone()
        }
    }

    #[test]
    fn resolve_returns_the_first_usable_source() {
        let sources: Vec<Box<dyn CredentialSource>> = vec![
            Box::new(Stub::err(SourceKind::File, CredentialError::NotFound)),
            Box::new(Stub::ok(SourceKind::SecurityCli, VALID)),
        ];
        let resolved = resolve(&sources).expect("should resolve");
        assert_eq!(resolved.source, SourceKind::SecurityCli);
        assert_eq!(resolved.credentials.access_token, "sk-ant-oat01-example");
    }

    #[test]
    fn resolve_skips_a_corrupt_source() {
        // A corrupt file must not mask a working keychain further down the
        // chain — this is the ordering that matters most on macOS.
        let sources: Vec<Box<dyn CredentialSource>> = vec![
            Box::new(Stub::ok(SourceKind::File, "{ corrupt")),
            Box::new(Stub::ok(SourceKind::SecurityCli, VALID)),
        ];
        let resolved = resolve(&sources).expect("should fall through");
        assert_eq!(resolved.source, SourceKind::SecurityCli);
    }

    #[test]
    fn resolve_reports_not_found_when_every_source_is_empty() {
        let sources: Vec<Box<dyn CredentialSource>> =
            vec![Box::new(Stub::err(SourceKind::File, CredentialError::NotFound))];
        assert_eq!(resolve(&sources).unwrap_err(), CredentialError::NotFound);
        assert_eq!(resolve(&[]).unwrap_err(), CredentialError::NotFound);
    }

    #[test]
    fn resolve_surfaces_a_real_failure_over_plain_absence() {
        // An unreadable source is more informative than "not found".
        let sources: Vec<Box<dyn CredentialSource>> = vec![
            Box::new(Stub::err(SourceKind::File, CredentialError::NotFound)),
            Box::new(Stub::err(
                SourceKind::SecurityCli,
                CredentialError::Unavailable("permission denied".into()),
            )),
        ];
        assert!(matches!(resolve(&sources).unwrap_err(), CredentialError::Unavailable(_)));
    }

    // MARK: Caching

    fn one_source(json: &str) -> Vec<Box<dyn CredentialSource>> {
        vec![Box::new(Stub::ok(SourceKind::File, json))]
    }

    #[test]
    fn cache_avoids_a_second_read() {
        // The whole reason the cache exists: on macOS every uncached read can
        // surface the ACL prompt.
        let stub = Stub::ok(SourceKind::File, VALID);
        let counter = stub.counter();
        let mut cache = CachedCredentials::new(vec![Box::new(stub)]);

        let now = 1_000_000_000_000;
        assert!(cache.token_at(now).is_ok());
        assert!(cache.token_at(now + 60_000).is_ok());
        assert!(cache.token_at(now + 120_000).is_ok());

        assert_eq!(counter.load(Ordering::SeqCst), 1, "should have read exactly once");
    }

    #[test]
    fn cache_refetches_once_the_token_expires() {
        let mut cache = CachedCredentials::new(one_source(VALID));
        let expiry = 1_788_512_886_774;

        assert!(cache.token_at(expiry - 1).is_ok());
        assert!(cache.is_authenticated_at(expiry - 1));

        // Past expiry the cache is not usable, so it re-reads. The stub returns
        // the same expired credential, so it stays unauthenticated. Well past
        // the refetch floor so the read is not suppressed.
        let later = expiry + CachedCredentials::DEFAULT_MIN_REFETCH_MS + 1;
        assert!(cache.token_at(later).is_ok());
        assert!(!cache.is_authenticated_at(later));
    }

    #[test]
    fn invalidate_forces_a_reread_after_the_floor() {
        let stub = Stub::ok(SourceKind::File, VALID);
        let counter = stub.counter();
        let mut cache = CachedCredentials::new(vec![Box::new(stub)]);

        let now = 1_000_000_000_000;
        assert!(cache.token_at(now).is_ok());
        cache.invalidate();
        assert!(cache.token_at(now + CachedCredentials::DEFAULT_MIN_REFETCH_MS + 1).is_ok());

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn repeated_invalidation_does_not_storm_the_source() {
        // A burst of 401s must collapse into a single read, or on macOS it
        // would become a burst of keychain prompts.
        let stub = Stub::ok(SourceKind::File, VALID);
        let counter = stub.counter();
        let mut cache = CachedCredentials::new(vec![Box::new(stub)]);

        let now = 1_000_000_000_000;
        assert!(cache.token_at(now).is_ok());
        for i in 0..20 {
            cache.invalidate();
            let _ = cache.token_at(now + i * 100);
        }

        // One initial read plus at most one more inside the floor window.
        let calls = counter.load(Ordering::SeqCst);
        assert!(calls <= 2, "expected the floor to collapse the burst, got {calls} reads");
    }

    #[test]
    fn plan_is_exposed_after_a_successful_read() {
        let mut cache = CachedCredentials::new(one_source(VALID));
        assert_eq!(cache.plan(), None, "nothing cached yet");
        assert!(cache.token_at(1_000_000_000_000).is_ok());
        assert_eq!(cache.plan(), Some("pro"));
    }

    #[test]
    fn is_not_authenticated_before_the_first_read() {
        let cache = CachedCredentials::new(one_source(VALID));
        assert!(!cache.is_authenticated_at(1_000_000_000_000));
        // Also true against the real clock — nothing is cached at all.
        assert!(!cache.is_authenticated());
    }

    // MARK: File source

    #[test]
    fn missing_file_is_not_found_not_an_error() {
        let src = file::FileSource::new("/nonexistent/path/.credentials.json");
        assert_eq!(src.load_json().unwrap_err(), CredentialError::NotFound);
    }

    #[test]
    fn file_source_reads_a_real_file() {
        let dir = std::env::temp_dir().join(format!("cu-cred-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(&path, VALID).unwrap();

        let src = file::FileSource::new(&path);
        let json = src.load_json().expect("should read");
        let creds = Credentials::from_json(&json).expect("should parse");
        assert_eq!(creds.subscription_type.as_deref(), Some("pro"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn source_labels_are_distinct() {
        // These strings go into diagnostics; a copy-paste collision would make
        // "which source won" useless.
        let labels = [
            SourceKind::File.label(),
            SourceKind::SecurityCli.label(),
            SourceKind::Keychain.label(),
        ];
        let mut unique = labels.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len());
    }
}
