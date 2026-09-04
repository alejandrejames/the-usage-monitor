//! Live end-to-end check of one usage poll and one status fetch.
//!
//!     cargo run -p claudeusage --bin pollcheck
//!
//! Exercises the whole chain outside the GUI: credential resolution, the
//! `/v1/messages` request, rate-limit header parsing, and Statuspage parsing.
//! Prints no token material.

use claudeusage::state::{poll_once, poll_status_once, AppState, PollOutcome};
use std::sync::Arc;

fn main() {
    let state = Arc::new(AppState::new());

    match poll_once(&state) {
        PollOutcome::Updated(s) => {
            println!("USAGE OK");
            println!("  session: {}%", s.session_percent);
            println!("  weekly:  {}%", s.weekly_percent);
            println!("  session reset ms: {:?}", s.session_reset_at_ms);
        }
        PollOutcome::Unauthenticated => {
            println!("UNAUTHENTICATED — run `claude` and log in");
            std::process::exit(1);
        }
        PollOutcome::Failed => {
            println!("FAILED — network or unexpected response");
            std::process::exit(2);
        }
    }

    println!("STATUS: {}", if poll_status_once(&state) { "ok" } else { "failed" });
    for service in state.snapshot().services {
        println!("  {:<14} {:?}", service.name, service.health);
    }
}
