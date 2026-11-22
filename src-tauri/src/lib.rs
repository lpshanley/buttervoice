mod hud;
mod key_tap;
mod messages;
mod microphone;
mod preferences;
mod saved_key;
mod transcribers;
mod typing;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, KeyDictations,
};
use buttervoice_core::language::Language;
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::Provider;
use buttervoice_replay::recordings;
use buttervoice_telemetry::{Build, BuildKind, Telemetry};
use hud::Hud;
use key_tap::{TapEnded, TapEvent};
use messages::Message;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use transcribers::Transcribers;

rust_i18n::i18n!("../locales", fallback = "en");

const KEY_LISTENER_RETRY: Duration = Duration::from_secs(2);

type Dictations = Arc<Mutex<KeyDictations>>;

fn listen_for_the_dictation_key(
    dictations: Dictations,
    hud: Arc<Hud>,
    transcribers: Arc<Transcribers>,
) -> std::io::Result<()> {
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
                    // Transcribing happens outside the lock, so key events
                    // are still handled while it runs.
                    let (provider, language, outcome) = transcribers.transcribe(&audio);
                    let after = lock(&dictations).transcription_finished(
                        provider,
                        language,
                        outcome,
                        key_tap::now(),
                    );
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
                    let (command, status, started) = {
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
                        let started = matches!(command, Some(CaptureCommand::Start))
                            .then(|| dictations.latest_dictation())
                            .flatten();
                        (command, dictations.status(), started)
                    };
                    // The microphone first: the HUD and the log can wait a
                    // moment, the first word cannot.
                    send(command);
                    hud.show(status);
                    if let Some(dictation) = started {
                        buttervoice_telemetry::record_dictation_started(dictation);
                    }
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

#[derive(serde::Serialize)]
struct TranscriptionSetup {
    provider: &'static str,
    state: &'static str,
}

#[tauri::command]
fn transcription_setup(transcribers: tauri::State<'_, Arc<Transcribers>>) -> TranscriptionSetup {
    let setup = transcribers.setup();
    TranscriptionSetup {
        provider: setup.provider.name(),
        state: setup.state,
    }
}

// `async` keeps loading a model, or a Keychain prompt, off the main thread.
#[tauri::command(async)]
fn choose_provider(
    provider: String,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    transcribers
        .choose_provider(Provider::from_setting(Some(&provider)))
        .map_err(|error| not_saved(error, &transcribers))
}

fn not_saved(error: std::io::Error, transcribers: &Transcribers) -> String {
    tracing::error!(%error, "a choice could not be saved");
    Message::ChoiceNotSaved.text(transcribers.display_language())
}

#[derive(serde::Serialize)]
struct LanguageSetup {
    language: &'static str,
    /// False until the person has picked one; the app then shows the
    /// language closest to the Mac's.
    chosen: bool,
}

#[tauri::command]
fn language_setup(transcribers: tauri::State<'_, Arc<Transcribers>>) -> LanguageSetup {
    LanguageSetup {
        language: transcribers.display_language().tag(),
        chosen: transcribers.language_chosen(),
    }
}

/// The windows are told through an event, so both change without a restart.
const DISPLAY_LANGUAGE_CHANGED: &str = "display-language";

#[tauri::command]
fn choose_language(
    language: String,
    app: tauri::AppHandle,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    let Some(language) = Language::from_setting(Some(&language)) else {
        return Err(Message::ChoiceNotSaved.text(transcribers.display_language()));
    };
    transcribers
        .choose_language(language)
        .map_err(|error| not_saved(error, &transcribers))?;
    if let Some(item) = app.tray_by_id(MENU_BAR_ITEM) {
        let menu = menu_in(&app, language).map_err(|error| error.to_string())?;
        item.set_menu(Some(menu))
            .map_err(|error| error.to_string())?;
    }
    app.emit(DISPLAY_LANGUAGE_CHANGED, language.tag())
        .map_err(|error| error.to_string())
}

// `async` keeps the Keychain call, which can wait on a system prompt, off
// the main thread.
#[tauri::command(async)]
fn save_api_key(
    key: String,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err(Message::EnterAKeyFirst.text(transcribers.display_language()));
    }
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

fn menu_in<R: tauri::Runtime, M: Manager<R>>(
    app: &M,
    language: Language,
) -> tauri::Result<Menu<R>> {
    let item = |id, message: Message| {
        MenuItem::with_id(app, id, message.text(language), true, None::<&str>)
    };
    let mark_bad = item(MARK_BAD, Message::MarkLastDictationAsBad)?;
    let settings = item(SETTINGS, Message::OpenSettings)?;
    let quit = item(QUIT, Message::Quit)?;
    Menu::with_items(app, &[&mark_bad, &settings, &quit])
}

const MENU_BAR_ITEM: &str = "menu-bar";

fn add_menu_bar_item(app: &tauri::App, language: Language) -> tauri::Result<()> {
    let menu = menu_in(app, language)?;
    let mut item = TrayIconBuilder::with_id(MENU_BAR_ITEM)
        .tooltip("ButterVoice")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT {
                app.exit(0);
            } else if event.id() == MARK_BAD {
                let latest = lock(&app.state::<Dictations>()).latest_dictation();
                buttervoice_telemetry::record_marked_bad(latest);
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

/// Not `settings.json`: the original app keeps a file of that name in the
/// same folder, in a different shape, and this one must not overwrite it.
const PREFERENCES_FILE: &str = "preferences.json";

/// The model the local provider runs, chosen by benchmark on real dictations.
const LOCAL_MODEL: &str = "parakeet-tdt-0.6b-v2-Q8_0.gguf";

const QUIT: &str = "quit";
const MARK_BAD: &str = "mark-bad";
/// Both the menu item and the window it opens.
const SETTINGS: &str = "settings";

/// The state behind the lock is only ever changed by whole method calls, so
/// it is still usable after another thread panicked while holding it.
fn lock(dictations: &Mutex<KeyDictations>) -> MutexGuard<'_, KeyDictations> {
    dictations.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Emptied when the app quits. Tauri ends the process without running
/// destructors, so nothing else would write out the last log lines.
struct RunningTelemetry(Mutex<Option<Telemetry>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .setup(|app| {
            let telemetry = buttervoice_telemetry::install(&app.path().app_log_dir()?)?;
            app.manage(RunningTelemetry(Mutex::new(Some(telemetry))));
            buttervoice_telemetry::record_app_started(&Build {
                version: env!("CARGO_PKG_VERSION"),
                kind: if cfg!(debug_assertions) {
                    BuildKind::Debug
                } else {
                    BuildKind::Release
                },
                commit: env!("BUTTERVOICE_COMMIT"),
            });
            // A menu-bar app: no Dock icon and no window of its own.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            buttervoice_local::silence_engine_log();
            let transcribers = Transcribers::start(
                keychain_service(&app.config().identifier),
                app.path().app_config_dir()?.join(PREFERENCES_FILE),
                app.path().app_data_dir()?.join("models").join(LOCAL_MODEL),
                // Until a person chooses, the language closest to the one
                // their Mac is set to stands in.
                Language::closest_to(&sys_locale::get_locale().unwrap_or_default()),
            );
            app.manage(transcribers.clone());
            add_menu_bar_item(app, transcribers.display_language())?;
            let hud = Arc::new(Hud::set_up(app.handle())?);
            let dictations: Dictations = Arc::new(Mutex::new(KeyDictations::new(
                HotkeyKey::RightOption,
                Mode::PushToTalk,
            )));
            app.manage(dictations.clone());
            listen_for_the_dictation_key(dictations, hud, transcribers)?;
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
            transcription_setup,
            language_setup,
            choose_language,
            choose_provider,
            save_api_key,
            forget_api_key
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                buttervoice_telemetry::record_app_quit();
                if let Some(running) = app.try_state::<RunningTelemetry>() {
                    running
                        .0
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                }
            }
        });
}
