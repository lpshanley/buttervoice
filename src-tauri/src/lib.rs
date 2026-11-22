mod hud;
mod key_tap;
mod microphone;
mod saved_key;
mod transcribers;
mod typing;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buttervoice_core::api_key::KeySource;
use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, KeyDictations,
};
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::TranscriptionFailure;
use buttervoice_replay::recordings;
use hud::Hud;
use key_tap::{TapEnded, TapEvent};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use transcribers::Transcribers;

const KEY_LISTENER_RETRY: Duration = Duration::from_secs(2);

fn listen_for_the_dictation_key(
    hud: Arc<Hud>,
    transcribers: Arc<Transcribers>,
) -> std::io::Result<()> {
    let dictations = Arc::new(Mutex::new(KeyDictations::new(
        HotkeyKey::RightOption,
        Mode::PushToTalk,
    )));
    let recordings_dir = std::env::var_os(RECORDINGS_VARIABLE).map(PathBuf::from);
    let microphone = microphone::spawn({
        let dictations = dictations.clone();
        let hud = hud.clone();
        move |captured| {
            let after = lock(&dictations).capture_stopped(captured, key_tap::now());
            let finished = match after {
                Some(AfterCapture::Transcribe(audio)) => {
                    if let Some(dir) = &recordings_dir {
                        save_for_replay(dir, &audio);
                    }
                    // The network call happens outside the lock, so key
                    // events are still handled while it runs.
                    let outcome = match transcribers.current() {
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
            hud.show(lock(&dictations).status());
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
            // The tap cannot be created until Input Monitoring is granted, so
            // keep trying: the grant then takes effect without a restart.
            let mut waiting_for_permission = false;
            loop {
                let ended = key_tap::run(|event| {
                    let (command, status) = {
                        let mut dictations = lock(&dictations);
                        let command = match event {
                            TapEvent::Ready { flags, at } => {
                                tracing::info!("key listener ready");
                                dictations.resync(flags, at)
                            }
                            TapEvent::FlagsChanged { flags, at } => {
                                dictations.flags_changed(flags, at)
                            }
                            TapEvent::Interrupted { flags, at } => {
                                tracing::warn!("key listener interrupted");
                                dictations.resync(flags, at)
                            }
                        };
                        (command, dictations.status())
                    };
                    // The microphone first: the HUD can wait a moment, the
                    // first word cannot.
                    send(command);
                    hud.show(status);
                });
                send(lock(&dictations).key_listener_lost(key_tap::now()));
                hud.show(lock(&dictations).status());
                let now_waiting = matches!(ended, TapEnded::Unavailable);
                if !(now_waiting && waiting_for_permission) {
                    tracing::error!(reason = %ended, "key listener stopped; retrying");
                }
                waiting_for_permission = now_waiting;
                std::thread::sleep(KEY_LISTENER_RETRY);
            }
        })?;
    Ok(())
}

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

#[tauri::command]
fn api_key_source(transcribers: tauri::State<'_, Arc<Transcribers>>) -> &'static str {
    match transcribers.key_source() {
        Some(KeySource::Environment) => "environment",
        Some(KeySource::Saved) => "saved",
        None => "none",
    }
}

// `async` keeps the Keychain call, which can wait on a system prompt, off
// the main thread.
#[tauri::command(async)]
fn save_api_key(
    key: String,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    transcribers.save_key(&key)
}

#[tauri::command(async)]
fn forget_api_key(transcribers: tauri::State<'_, Arc<Transcribers>>) -> Result<(), String> {
    transcribers.forget_key()
}

/// Debug builds keep their key in an entry of their own. An unsigned build
/// is a different program to macOS after every rebuild, so sharing one entry
/// with the installed app makes each of them prompt for the other's.
fn keychain_service(identifier: &str) -> String {
    if cfg!(debug_assertions) {
        format!("{identifier}.dev")
    } else {
        identifier.to_string()
    }
}

fn add_menu_bar_item(app: &tauri::App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, SETTINGS, "Settings\u{2026}", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit ButterVoice", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;
    let mut item = TrayIconBuilder::new()
        .tooltip("ButterVoice")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT {
                app.exit(0);
            } else if event.id() == SETTINGS
                && let Some(window) = app.get_webview_window(SETTINGS)
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    if let Some(icon) = app.default_window_icon() {
        item = item.icon(icon.clone());
    }
    item.build(app)?;
    Ok(())
}

const QUIT: &str = "quit";
/// Both the menu item and the window it opens.
const SETTINGS: &str = "settings";

/// The state behind the lock is only ever changed by whole method calls, so
/// it is still usable after another thread panicked while holding it.
fn lock(dictations: &Mutex<KeyDictations>) -> MutexGuard<'_, KeyDictations> {
    dictations.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .setup(|app| {
            let log_file_guard = buttervoice_telemetry::install(&app.path().app_log_dir()?)?;
            app.manage(log_file_guard);
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "app started");
            // A menu-bar app: no Dock icon and no window of its own.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            add_menu_bar_item(app)?;
            let hud = Arc::new(Hud::set_up(app.handle())?);
            let transcribers = Transcribers::start(keychain_service(&app.config().identifier));
            app.manage(transcribers.clone());
            listen_for_the_dictation_key(hud, transcribers)?;
            Ok(())
        })
        // Closing the settings window only hides it, so it can be opened again.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == SETTINGS
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            api_key_source,
            save_api_key,
            forget_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
