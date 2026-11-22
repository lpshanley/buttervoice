mod key_tap;
mod microphone;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{AfterCapture, CaptureCommand, KeyDictations};
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::{Transcription, TranscriptionFailure};
use buttervoice_grok::GrokTranscriber;
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
    let microphone = microphone::spawn({
        let dictations = dictations.clone();
        move |captured| {
            let after = lock(&dictations).capture_stopped(captured, key_tap::now());
            let finished = match after {
                Some(AfterCapture::Transcribe(audio)) => {
                    // The network call happens outside the lock, so key
                    // events are still handled while it runs.
                    let outcome = match &transcriber {
                        Some(transcriber) => transcriber.transcribe(&audio),
                        None => Err(TranscriptionFailure::NotConfigured),
                    };
                    lock(&dictations).transcription_finished(outcome, key_tap::now())
                }
                Some(AfterCapture::Finished(finished)) => Some(finished),
                None => None,
            };
            if let Some(finished) = finished {
                buttervoice_telemetry::record_dictation(&finished);
                show_transcript_in_dev_builds(&finished.transcription);
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

/// Until text is typed into the focused app, the terminal running a debug
/// build is the only place a transcript can be seen. It never goes to the
/// log file.
fn show_transcript_in_dev_builds(transcription: &Transcription) {
    if cfg!(debug_assertions)
        && let Transcription::Succeeded(transcript) = transcription
    {
        eprintln!("transcript: {}", transcript.text);
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
