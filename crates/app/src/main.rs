// Prevents a console window alongside the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! ClaudeUsage — cross-platform tray monitor for Claude subscription usage.
//!
//! Replaces the SwiftUI `MenuBarExtra` host from `ClaudeUsageApp.swift`.

mod state;
mod tray;

use claudeusage_core::{status, Alert};
use state::{AppSnapshot, AppState, PollOutcome};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;

/// Event the WebView listens on for state changes.
const SNAPSHOT_EVENT: &str = "usage://snapshot";

// MARK: - Commands

/// Current state, for the popover's initial render.
#[tauri::command]
fn get_snapshot(state: tauri::State<'_, Arc<AppState>>) -> AppSnapshot {
    state.inner().snapshot()
}

/// Forces an immediate poll, for the popover's Refresh button.
#[tauri::command]
fn refresh_now(app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) {
    let state = Arc::clone(state.inner());
    std::thread::spawn(move || {
        run_poll(&app, &state);
    });
}

/// Re-checks for a credential, for the "Claude Code not detected" prompt.
#[tauri::command]
fn recheck_credentials(app: tauri::AppHandle, state: tauri::State<'_, Arc<AppState>>) {
    state.inner().credentials.lock().expect("credential lock").invalidate();
    let state = Arc::clone(state.inner());
    std::thread::spawn(move || {
        run_poll(&app, &state);
    });
}

// MARK: - Polling

/// One usage poll, applying the result and notifying the UI and tray.
fn run_poll(app: &tauri::AppHandle, state: &Arc<AppState>) -> bool {
    let succeeded = match state::poll_once(state) {
        PollOutcome::Updated(snapshot) => {
            if let Some(alert) = state::apply_usage(state, snapshot) {
                send_alert(app, alert);
            }
            true
        }
        PollOutcome::Unauthenticated => {
            state::mark_stale(state, false);
            false
        }
        PollOutcome::Failed => {
            state::mark_stale(state, true);
            false
        }
    };

    publish(app, state);
    succeeded
}

/// Pushes the current snapshot to both the tray and the WebView.
fn publish(app: &tauri::AppHandle, state: &Arc<AppState>) {
    let snapshot = state.snapshot();
    if let Some(tray) = app.tray_by_id("main") {
        tray::update(&tray, &snapshot);
    }
    let _ = app.emit(SNAPSHOT_EVENT, &snapshot);
}

fn send_alert(app: &tauri::AppHandle, alert: Alert) {
    let _ = app.notification().builder().title(alert.title()).body(alert.body()).show();
}

/// Background thread: usage polling with backoff on failure.
fn spawn_usage_poller(app: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut failures: u32 = 0;
        loop {
            if run_poll(&app, &state) {
                failures = 0;
            } else {
                failures = failures.saturating_add(1);
            }
            std::thread::sleep(state::next_delay(failures));
        }
    });
}

/// Background thread: service status on its own slow cadence, independent of
/// usage so a failing status fetch never affects the usage reading.
fn spawn_status_poller(app: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        state::poll_status_once(&state);
        let _ = app.emit(SNAPSHOT_EVENT, &state.snapshot());
        std::thread::sleep(std::time::Duration::from_secs(status::POLL_INTERVAL_SECS));
    });
}

// MARK: - Entry point

fn main() {
    tauri::Builder::default()
        // A tray app must never run twice; a second launch just shows the
        // existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![get_snapshot, refresh_now, recheck_credentials])
        .setup(|app| {
            let state = Arc::new(AppState::new());
            app.manage(Arc::clone(&state));

            // macOS: without Accessory the app shows a Dock icon and steals
            // focus. This is the direct replacement for MenuBarExtra's
            // implicit behaviour.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit])?;

            TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_popover(tray.app_handle());
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            publish(&handle, &state);
            spawn_usage_poller(handle.clone(), Arc::clone(&state));
            spawn_status_poller(handle, state);

            Ok(())
        })
        .on_window_event(|window, event| {
            // Auto-hide on focus loss, matching the menu-bar panel behaviour.
            // Hiding rather than closing keeps the app resident in the tray.
            if let WindowEvent::Focused(false) = event {
                note_hidden_by_focus_loss();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build ClaudeUsage")
        .run(|_app, event| {
            // Closing the popover must not quit: the app lives in the tray.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}

// MARK: - Popover visibility

/// When the popover was last hidden by losing focus, as millis since start.
///
/// Clicking the tray icon while the popover is open delivers focus-loss
/// *first* and the tray click second. Without this the click would reopen the
/// window that the focus-loss just closed, so the popover appears never to
/// close. Windows is where this is most pronounced, but the ordering is not
/// guaranteed anywhere, so the guard is unconditional.
static LAST_FOCUS_HIDE_MS: AtomicU64 = AtomicU64::new(0);

/// How long after a focus-loss hide a tray click is treated as part of the
/// same gesture. Long enough to cover the event gap, short enough that a
/// deliberate second click still opens the popover.
const CLICK_DEBOUNCE_MS: u64 = 250;

/// Process-relative clock. `Instant` cannot be stored in an atomic, and a
/// monotonic counter avoids wall-clock jumps entirely.
fn uptime_millis() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn note_hidden_by_focus_loss() {
    LAST_FOCUS_HIDE_MS.store(uptime_millis(), Ordering::SeqCst);
}

/// True when a tray click arrived so soon after a focus-loss hide that it is
/// the same user gesture.
fn click_closed_the_popover() -> bool {
    let last = LAST_FOCUS_HIDE_MS.load(Ordering::SeqCst);
    last != 0 && uptime_millis().saturating_sub(last) < CLICK_DEBOUNCE_MS
}

/// Shows or hides the popover.
fn toggle_popover(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // The focus-loss handler already hid it; this click ends the gesture.
    if click_closed_the_popover() {
        return;
    }

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
