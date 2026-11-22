mod key_tap;

use buttervoice_core::capture::CaptureReport;
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
    std::thread::Builder::new()
        .name("key-tap".into())
        .spawn(|| {
            let mut dictations = KeyDictations::new(HotkeyKey::RightOption, Mode::PushToTalk);
            let ended = key_tap::run(|event| {
                let command = match event {
                    TapEvent::FlagsChanged { flags, at } => dictations.flags_changed(flags, at),
                    TapEvent::Interrupted { flags, at } => {
                        tracing::warn!("key listener interrupted");
                        dictations.resync(flags, at)
                    }
                };
                carry_out(command, &mut dictations);
            });
            let command = dictations.key_listener_lost(key_tap::now());
            carry_out(command, &mut dictations);
            tracing::error!(reason = %ended, "key listener stopped");
        })?;
    Ok(())
}

fn carry_out(command: Option<CaptureCommand>, dictations: &mut KeyDictations) {
    // No microphone yet: capture "stops" at once with nothing captured.
    if command == Some(CaptureCommand::Stop)
        && let Some(finished) = dictations.capture_stopped(CaptureReport::default())
    {
        buttervoice_telemetry::record_dictation(&finished.summary, finished.stopped_because);
    }
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
