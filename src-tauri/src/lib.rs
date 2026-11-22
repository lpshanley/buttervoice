mod key_tap;
mod microphone;
mod typing;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, KeyDictations,
};
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_grok::GrokTranscriber;
use buttervoice_replay::recordings;
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
    let transcriber = grok_transcriber();
    let recordings_dir = std::env::var_os(RECORDINGS_VARIABLE).map(PathBuf::from);
    let microphone = microphone::spawn({
        let dictations = dictations.clone();
        move |captured| {
            let after = lock(&dictations).capture_stopped(captured, key_tap::now());
            let finished = match after {
                Some(AfterCapture::Transcribe(audio)) => {
                    if let Some(dir) = &recordings_dir {
                        save_for_replay(dir, &audio);
                    }
                    // The network call happens outside the lock, so key
                    // events are still handled while it runs.
                    let outcome = match &transcriber {
                        Some(transcriber) => {
                            transcriber.transcribe(&audio.boosted_for_transcription())
                        }
                        None => Err(TranscriptionFailure::NotConfigured),
                    };
                    let after = lock(&dictations).transcription_finished(outcome, key_tap::now());
                    match after {
                        Some(AfterTranscription::Type(text)) => {
                            let typed = typing::type_text(&text);
                            lock(&dictations).typing_finished(typed, key_tap::now())
                        }
                        Some(AfterTranscription::Finished(finished)) => Some(finished),
                        None => None,
                    }
                }
                Some(AfterCapture::Finished(finished)) => Some(finished),
                None => None,
            };
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

const API_KEY_VARIABLE: &str = "XAI_API_KEY";
/// When set, every dictation's audio is saved into this folder for the
/// replay tool. Meant for development: it writes a real voice to disk.
const RECORDINGS_VARIABLE: &str = "BUTTERVOICE_RECORDINGS_DIR";

fn save_for_replay(dir: &Path, audio: &CapturedAudio) {
    let number = recordings::next_number(dir);
    match recordings::save(dir, number, audio) {
        Ok(_) => tracing::info!(number, "recording saved for replay"),
        Err(error) => tracing::warn!(%error, "recording could not be saved for replay"),
    }
}

fn grok_transcriber() -> Option<GrokTranscriber> {
    let Some(api_key) = std::env::var(API_KEY_VARIABLE)
        .ok()
        .filter(|key| !key.trim().is_empty())
    else {
        tracing::warn!(
            variable = API_KEY_VARIABLE,
            "no xAI API key in the environment; dictations will not be transcribed"
        );
        return None;
    };
    match GrokTranscriber::new(buttervoice_grok::DEFAULT_BASE_URL, api_key) {
        Ok(transcriber) => Some(transcriber),
        Err(error) => {
            tracing::error!(%error, "the transcription client could not be set up");
            None
        }
    }
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
