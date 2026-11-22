mod key_tap;
mod microphone;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{CaptureCommand, KeyDictations};
use buttervoice_core::recording::Mode;
use key_tap::TapEvent;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

fn listen_for_the_dictation_key() -> std::io::Result<()> {
    let dictations = Arc::new(Mutex::new(KeyDictations::new(
        HotkeyKey::RightOption,
        Mode::PushToTalk,
    )));
    let microphone = microphone::spawn({
        let dictations = dictations.clone();
        move |captured| {
            let finished = lock(&dictations).capture_stopped(captured);
            if let Some(finished) = finished {
                buttervoice_telemetry::record_dictation(&finished);
            }
        }
    })?;
    std::thread::Builder::new()
        .name("key-tap".into())
        .spawn(move || {
            let send = |command: Option<CaptureCommand>| {
                if let Some(command) = command {
                    let _ = microphone.send(command);
                }
            };
            let ended = key_tap::run(|event| {
                let command = match event {
                    TapEvent::FlagsChanged { flags, at } => {
                        lock(&dictations).flags_changed(flags, at)
                    }
                    TapEvent::Interrupted { flags, at } => {
                        tracing::warn!("key listener interrupted");
                        lock(&dictations).resync(flags, at)
                    }
                };
                send(command);
            });
            send(lock(&dictations).key_listener_lost(key_tap::now()));
            tracing::error!(reason = %ended, "key listener stopped");
        })?;
    Ok(())
}

/// The state behind the lock is only ever changed by whole method calls, so
/// it is still usable after another thread panicked while holding it.
fn lock(dictations: &Mutex<KeyDictations>) -> MutexGuard<'_, KeyDictations> {
    dictations.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let log_file_guard = buttervoice_telemetry::install(&app.path().app_log_dir()?)?;
            app.manage(log_file_guard);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "app started");
            listen_for_the_dictation_key()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
