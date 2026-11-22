mod hud;
mod key_tap;
mod messages;
mod microphone;
mod preferences;
mod samples;
mod saved_key;
mod transcribers;
mod typing;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::capture::{InputKind, MicrophoneChoice};
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, KeyDictations,
};
use buttervoice_core::language::Language;
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::Provider;
use buttervoice_local::fetch::FetchError;
use buttervoice_replay::recordings;
use buttervoice_samples::RecordedWith;
use buttervoice_samples::session::Take;
use buttervoice_telemetry::{Build, BuildKind, Telemetry};
use hud::Hud;
use key_tap::{TapEnded, TapEvent};
use messages::Message;
use samples::Recorder;
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
    recorder: Arc<Recorder>,
    preferences: Arc<preferences::Store>,
    app: tauri::AppHandle,
) -> std::io::Result<()> {
    let recordings_dir = std::env::var_os(RECORDINGS_VARIABLE).map(PathBuf::from);
    let microphone = microphone::spawn(
        {
            let preferences = preferences.clone();
            move || preferences.get().microphone
        },
        {
            let dictations = dictations.clone();
            let hud = hud.clone();
            move |captured| {
                let input = captured.as_ref().map(|captured| captured.input);
                let after = lock(&dictations).capture_stopped(captured, key_tap::now());
                let finished = match after {
                    // While samples are being recorded, a dictation is a take
                    // for the session and is never typed anywhere.
                    Some(AfterCapture::Transcribe(audio)) if taking_samples(&recorder, &app) => {
                        let hint = transcribers.heard_on_this_mac(&audio);
                        let heard = match &hint {
                            Some((_, _, Ok(transcript))) => Some(transcript.text.clone()),
                            _ => None,
                        };
                        recorder.recorded(Take {
                            audio,
                            input: input.unwrap_or(InputKind::Other),
                            heard,
                        });
                        let _ = app.emit_to(SAMPLES, SAMPLES_CHANGED, ());
                        match hint {
                            Some((provider, language, outcome)) => lock(&dictations)
                                .transcription_kept(provider, language, outcome, key_tap::now()),
                            None => lock(&dictations).kept_untranscribed(),
                        }
                    }
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
        },
    )?;
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
    model_bytes: Option<u64>,
}

#[tauri::command]
fn transcription_setup(transcribers: tauri::State<'_, Arc<Transcribers>>) -> TranscriptionSetup {
    let setup = transcribers.setup();
    TranscriptionSetup {
        provider: setup.provider.name(),
        state: setup.state,
        model_bytes: setup.model_bytes,
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

// `async`: a new language can mean loading a different model.
#[tauri::command(async)]
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

#[tauri::command]
fn samples_state(recorder: tauri::State<'_, Arc<Recorder>>) -> samples::State {
    recorder.state()
}

#[tauri::command]
fn samples_start(
    speaker: String,
    recorder: tauri::State<'_, Arc<Recorder>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) {
    let recorded_with = RecordedWith {
        speaker,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        app_commit: env!("BUTTERVOICE_COMMIT").to_string(),
        chip: what_the_system_says("machdep.cpu.brand_string"),
        macos: what_the_system_says("kern.osproductversion"),
    };
    recorder.start(recorded_with, transcribers.dictation_language());
}

fn what_the_system_says(name: &str) -> String {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", name])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[tauri::command]
fn samples_next(said: String, recorder: tauri::State<'_, Arc<Recorder>>) {
    recorder.next(&said);
}

#[tauri::command]
fn samples_skip(recorder: tauri::State<'_, Arc<Recorder>>) {
    recorder.skip();
}

#[tauri::command]
fn samples_back(recorder: tauri::State<'_, Arc<Recorder>>) {
    recorder.back();
}

/// The raw WAV bytes, played by the window without being written to disk.
#[tauri::command]
fn samples_audio(recorder: tauri::State<'_, Arc<Recorder>>) -> tauri::ipc::Response {
    tauri::ipc::Response::new(recorder.audio().unwrap_or_default())
}

#[tauri::command]
fn samples_cancel(recorder: tauri::State<'_, Arc<Recorder>>) {
    recorder.cancel();
}

/// Returns the folder the set was written to.
#[tauri::command(async)]
fn samples_finish(
    recorder: tauri::State<'_, Arc<Recorder>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<String, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    recorder
        .finish(now)
        .map(|folder| folder.display().to_string())
        .map_err(|error| {
            tracing::error!(%error, "the sample set could not be saved");
            Message::SamplesNotSaved.text(transcribers.display_language())
        })
}

/// How far a model download has got, sent to the settings window.
const MODEL_DOWNLOAD: &str = "model-download";

/// Progress events are sent at most this often: a model arrives in tens of
/// thousands of reads.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

// `async`: the download takes minutes and must not hold the main thread.
#[tauri::command(async)]
fn download_model(
    app: tauri::AppHandle,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    let mut last_sent = std::time::Instant::now() - PROGRESS_EVERY;
    let outcome = transcribers.download_model(|bytes| {
        if last_sent.elapsed() >= PROGRESS_EVERY {
            last_sent = std::time::Instant::now();
            let _ = app.emit_to(SETTINGS, MODEL_DOWNLOAD, bytes);
        }
    });
    let language = transcribers.display_language();
    outcome.map_err(|error| {
        match error {
            FetchError::Cancelled => Message::DownloadCancelled,
            FetchError::Interrupted => Message::DownloadInterrupted,
            FetchError::ChecksumMismatch => Message::DownloadDamaged,
            FetchError::Disk(_) => Message::DownloadNotSaved,
            FetchError::Unreachable | FetchError::Refused(_) => Message::DownloadUnreachable,
        }
        .text(language)
    })
}

#[tauri::command]
fn cancel_model_download(transcribers: tauri::State<'_, Arc<Transcribers>>) {
    transcribers.cancel_download();
}

#[derive(serde::Serialize)]
struct MicrophoneSetup {
    choice: String,
    connected: Vec<microphone::Microphone>,
}

// `async`: listing devices asks CoreAudio, which can take a moment.
#[tauri::command(async)]
fn microphone_setup(preferences: tauri::State<'_, Arc<preferences::Store>>) -> MicrophoneSetup {
    MicrophoneSetup {
        choice: preferences.get().microphone.to_setting(),
        connected: microphone::connected(),
    }
}

#[tauri::command]
fn choose_microphone(
    choice: String,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    let choice = MicrophoneChoice::from_setting(Some(&choice));
    preferences
        .change(|saved| saved.microphone = choice)
        .map_err(|error| not_saved(error, &transcribers))
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
    let record_samples = item(SAMPLES, Message::RecordSamples)?;
    let settings = item(SETTINGS, Message::OpenSettings)?;
    let quit = item(QUIT, Message::Quit)?;
    Menu::with_items(app, &[&mark_bad, &record_samples, &settings, &quit])
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
            } else if (event.id() == SETTINGS || event.id() == SAMPLES)
                && let Some(window) = app.get_webview_window(event.id().as_ref())
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

/// A session only takes dictations while its window is showing. With the
/// window closed part-way through, dictation goes back to typing and the
/// session waits.
fn taking_samples(recorder: &Recorder, app: &tauri::AppHandle) -> bool {
    recorder.active()
        && app
            .get_webview_window(SAMPLES)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
}

/// Both the menu item and the window it opens.
const SAMPLES: &str = "samples";
const SAMPLES_CHANGED: &str = "samples-changed";

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
            let preferences = Arc::new(preferences::Store::open(
                app.path().app_config_dir()?.join(PREFERENCES_FILE),
            ));
            let transcribers = Transcribers::start(
                keychain_service(&app.config().identifier),
                preferences.clone(),
                app.path().app_data_dir()?.join("models"),
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
            let recorder = Arc::new(Recorder::new(app.path().app_data_dir()?.join("samples")));
            app.manage(recorder.clone());
            listen_for_the_dictation_key(
                dictations,
                hud,
                transcribers,
                recorder,
                preferences.clone(),
                app.handle().clone(),
            )?;
            app.manage(preferences);
            Ok(())
        })
        // Closing the settings window only hides it, so it can be opened again.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && (window.label() == SETTINGS || window.label() == SAMPLES)
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            transcription_setup,
            language_setup,
            microphone_setup,
            download_model,
            cancel_model_download,
            choose_microphone,
            samples_state,
            samples_start,
            samples_next,
            samples_skip,
            samples_back,
            samples_audio,
            samples_cancel,
            samples_finish,
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
