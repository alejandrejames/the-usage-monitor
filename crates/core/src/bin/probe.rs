//! Credential probe — verifies the Phase 2 chain on any platform.
//!
//!     cargo run -p claudeusage-core --bin probe
//!
//! Reports which source produced a credential, the plan, and the expiry. It
//! deliberately prints **no** token material: not the token, not a prefix, not
//! a hash. The credential blob also carries unrelated MCP OAuth tokens, so the
//! only safe thing to show is metadata.
//!
//! Exit codes: 0 found, 1 not found, 2 environment error.

use claudeusage_core::credentials::{default_sources, file, resolve, CredentialError};

fn main() {
    println!("ClaudeUsage credential probe");
    println!("platform: {}", std::env::consts::OS);

    match file::default_credentials_path() {
        Some(path) => {
            let exists = path.exists();
            println!(
                "file path: {} ({})",
                path.display(),
                if exists { "exists" } else { "absent" }
            );
        }
        None => println!("file path: could not resolve a home directory"),
    }

    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        println!("CLAUDE_CONFIG_DIR: {}", dir.to_string_lossy());
    }

    let sources = default_sources();
    println!("\nsources, in order:");
    for (i, source) in sources.iter().enumerate() {
        println!("  {}. {}", i + 1, source.kind().label());
    }

    // Probe each source individually so a failure part-way down the chain is
    // visible, not just the winner.
    //
    // Stops at the first success on purpose. Probing every source would reach
    // the native keychain API, which blocks on the macOS ACL dialog — in a
    // non-interactive shell that hangs forever with nobody to click it. Pass
    // --all to probe past the winner anyway.
    let probe_all = std::env::args().any(|a| a == "--all");
    println!("\nper-source result:");
    for source in &sources {
        match source.load_json() {
            Ok(json) => {
                println!("  {:<18} ok ({} bytes)", source.kind().label(), json.len());
                if !probe_all {
                    println!("  {:<18} (later sources not probed; pass --all to force)", "");
                    break;
                }
            }
            Err(e) => println!("  {:<18} {e}", source.kind().label()),
        }
    }

    println!();
    match resolve(&sources) {
        Ok(resolved) => {
            println!("RESOLVED via: {}", resolved.source.label());
            println!(
                "  plan:    {}",
                resolved.credentials.subscription_type.as_deref().unwrap_or("unknown")
            );
            match resolved.credentials.expires_at {
                Some(ms) => {
                    let status =
                        if resolved.credentials.is_expired() { "EXPIRED" } else { "valid" };
                    println!("  expires: {ms} ms since epoch ({status})");
                }
                None => println!("  expires: not set (treated as non-expiring)"),
            }
            // Length only — never the value, not even a prefix.
            println!("  token:   present, {} chars", resolved.credentials.access_token.len());
        }
        Err(CredentialError::NotFound) => {
            eprintln!("NOT FOUND — run `claude` and log in, then re-run this probe.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    }
}
