mod clipboard;
mod frontmost;
mod hud;
mod key_tap;
mod messages;
mod microphone;
mod preferences;
mod samples;
mod saved_key;
mod transcribers;
mod typing;
mod updates;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use buttervoice_core::capture::{InputKind, MicrophoneChoice};
use buttervoice_core::hotkey::HotkeyKey;
use buttervoice_core::key_dictation::{
    AfterCapture, AfterTranscription, CaptureCommand, DictationStatus, FinishedDictation,
    KeyDictations, Purpose,
};
use buttervoice_core::language::Language;
use buttervoice_core::recording::Mode;
use buttervoice_core::transcription::Provider;
use buttervoice_core::typing::Typing;
use buttervoice_core::update_schedule::check_due;
use buttervoice_history::History;
use buttervoice_history::usage::{self, Usage};
use buttervoice_local::fetch::FetchError;
use buttervoice_samples::RecordedWith;
use buttervoice_samples::session::Take;
use buttervoice_telemetry::{Build, BuildKind, Telemetry};
use hud::Hud;
use key_tap::{TapEnded, TapEvent};
use messages::Message;
use samples::Recorder;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use transcribers::{Transcribed, Transcribers};
use updates::Updates;

rust_i18n::i18n!("../locales", fallback = "en");

const KEY_LISTENER_RETRY: Duration = Duration::from_secs(2);

type Dictations = Arc<Mutex<KeyDictations>>;

fn listen_for_the_dictation_key(
    dictations: Dictations,
    hud: Arc<Hud>,
    transcribers: Arc<Transcribers>,
    recorder: Arc<Recorder>,
    preferences: Arc<preferences::Store>,
    kept: Kept,
    app: tauri::AppHandle,
) -> std::io::Result<CaptureCommands> {
    let menu_app = app.clone();
    let keep_history = {
        let preferences = preferences.clone();
        move || preferences.get().keep_history
    };
    let purpose_now = {
        let recorder = recorder.clone();
        let preferences = preferences.clone();
        move || recorder.purpose_for(preferences.get().provider)
    };
    let drops_fillers_now = {
        let preferences = preferences.clone();
        move || preferences.get().drop_filler_words
    };
    let microphone = microphone::spawn(
        {
            let preferences = preferences.clone();
            move || preferences.get().microphone
        },
        {
            let dictations = dictations.clone();
            let hud = hud.clone();
            move || {
                let status = {
                    let mut dictations = lock(&dictations);
                    dictations.sound_arrived();
                    dictations.status()
                };
                hud.show(status);
            }
        },
        {
            let dictations = dictations.clone();
            let hud = hud.clone();
            move |captured| {
                let input = captured.as_ref().map(|captured| captured.input);
                let after = lock(&dictations).capture_stopped(captured, key_tap::now());
                let finished = match after {
                    Some(AfterCapture::Transcribe(audio, Purpose::SampleTake)) => {
                        let hint = transcribers.heard_on_this_mac(&audio);
                        let heard = match &hint {
                            Some(Transcribed {
                                outcome: Ok(transcript),
                                ..
                            }) => Some(transcript.text.clone()),
                            _ => None,
                        };
                        recorder.recorded(Take {
                            audio,
                            input: input.unwrap_or(InputKind::Other),
                            heard,
                        });
                        let _ = app.emit_to(SAMPLES, SAMPLES_CHANGED, ());
                        match hint {
                            Some(heard) => lock(&dictations).transcription_kept(
                                heard.provider,
                                heard.backend,
                                heard.language,
                                heard.outcome,
                                key_tap::now(),
                            ),
                            None => lock(&dictations).kept_untranscribed(),
                        }
                    }
                    Some(AfterCapture::Transcribe(audio, purpose)) => {
                        // Transcribing happens outside the lock, so key events
                        // are still handled while it runs.
                        let heard = transcribers.transcribe(&audio, purpose);
                        let after = lock(&dictations).transcription_finished(
                            heard.provider,
                            heard.backend,
                            heard.language,
                            heard.outcome,
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
                if let Some(FinishedDictation {
                    typing: Typing::Failed(_),
                    ..
                }) = &finished
                {
                    hud.not_typed();
                }
                if let Some(finished) = finished {
                    buttervoice_telemetry::record_dictation(&finished);
                    kept.record(&finished, keep_history());
                    let _ = app.emit_to(MAIN_WINDOW, DICTATION_FINISHED, ());
                }
            }
        },
    )?;
    let commands = CaptureCommands(microphone.clone());
    std::thread::Builder::new()
        .name("key-tap".into())
        .spawn(move || {
            // Posted, not waited for: the key thread never waits on the
            // main thread.
            let refresh_menu_for = |before: DictationStatus, after: DictationStatus| {
                if capturing(before) != capturing(after) {
                    let app = menu_app.clone();
                    let _ = menu_app.run_on_main_thread(move || refresh_menu(&app));
                }
            };
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
                    let (command, before, status, started, new_deadline) = {
                        let mut dictations = lock(&dictations);
                        let before = dictations.status();
                        let deadline_before = dictations.second_tap_deadline();
                        dictations.set_purpose(purpose_now());
                        dictations.set_drops_fillers(drops_fillers_now());
                        let command = match event {
                            TapEvent::Ready { flags, at } => {
                                tracing::info!("key listener ready");
                                dictations.resync(flags, at)
                            }
                            TapEvent::FlagsChanged { flags, at } => {
                                dictations.flags_changed(flags, at)
                            }
                            TapEvent::OtherKeyDown => {
                                dictations.other_key_pressed();
                                None
                            }
                            TapEvent::Interrupted { flags, at } => {
                                tracing::warn!("key listener interrupted");
                                dictations.resync(flags, at)
                            }
                        };
                        let started = matches!(command, Some(CaptureCommand::Start))
                            .then(|| dictations.latest_dictation())
                            .flatten();
                        let new_deadline = dictations
                            .second_tap_deadline()
                            .filter(|deadline| Some(*deadline) != deadline_before);
                        (command, before, dictations.status(), started, new_deadline)
                    };
                    // The microphone first: the HUD and the log can wait a
                    // moment, the first word cannot.
                    send(command);
                    hud.show(status);
                    refresh_menu_for(before, status);
                    if let Some(dictation) = started {
                        buttervoice_telemetry::record_dictation_started(dictation);
                    }
                    if let Some(deadline) = new_deadline {
                        close_second_tap_window_at(
                            deadline,
                            dictations.clone(),
                            microphone.clone(),
                            hud.clone(),
                            menu_app.clone(),
                        );
                    }
                });
                let before = lock(&dictations).status();
                send(lock(&dictations).key_listener_lost(key_tap::now()));
                let status = lock(&dictations).status();
                hud.show(status);
                refresh_menu_for(before, status);
                let now_waiting = matches!(ended, TapEnded::Unavailable);
                if !(now_waiting && waiting_for_permission) {
                    tracing::error!(reason = %ended, "key listener stopped; retrying");
                }
                waiting_for_permission = now_waiting;
                std::thread::sleep(KEY_LISTENER_RETRY);
            }
        })?;
    Ok(commands)
}

/// A single tap keeps capturing in case a second one follows; once that
/// can no longer happen, the dictation stops.
fn close_second_tap_window_at(
    deadline: buttervoice_core::timeline::Timestamp,
    dictations: Dictations,
    microphone: std::sync::mpsc::Sender<CaptureCommand>,
    hud: Arc<Hud>,
    app: tauri::AppHandle,
) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_nanos(
            deadline
                .as_nanos()
                .saturating_sub(key_tap::now().as_nanos()),
        ));
        let (command, before, status) = {
            let mut dictations = lock(&dictations);
            let before = dictations.status();
            let command = dictations.second_tap_window_closed(key_tap::now());
            (command, before, dictations.status())
        };
        if let Some(command) = command {
            let _ = microphone.send(command);
        }
        hud.show(status);
        if capturing(before) != capturing(status) {
            let menu_app = app.clone();
            let _ = app.run_on_main_thread(move || refresh_menu(&menu_app));
        }
    });
}

/// What is kept of each finished dictation.
struct Kept {
    history: Arc<History>,
    usage: Arc<Usage>,
}

impl Kept {
    /// The counts are always kept; the text only while the history is on.
    fn record(&self, finished: &FinishedDictation, keep_history: bool) {
        if keep_history
            && let Some(entry) =
                buttervoice_history::entry_for(finished, seconds_since_1970(), frontmost::app())
            && let Err(error) = self.history.add(&entry)
        {
            tracing::warn!(%error, "the dictation could not be added to the history");
        }
        if let Err(error) = self.usage.count(&local_date(0), finished) {
            tracing::warn!(%error, "the dictation could not be counted");
        }
    }
}

/// Where commands for the microphone go, for the menu's Stop Dictation.
struct CaptureCommands(std::sync::mpsc::Sender<CaptureCommand>);

fn capturing(status: DictationStatus) -> bool {
    matches!(
        status,
        DictationStatus::Starting | DictationStatus::Listening
    )
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

/// The webviews are told through an event so they change without a restart.
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
    refresh_window_titles(&app, language);
    refresh_menu(&app);
    if let Err(error) = app.emit(DISPLAY_LANGUAGE_CHANGED, language.tag()) {
        tracing::warn!(%error, "the webviews could not be notified of the language change");
    }
    Ok(())
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
    transcribers.save_key(&key).map_err(|error| {
        tracing::warn!(%error, "the API key could not be saved");
        Message::KeyNotSaved.text(transcribers.display_language())
    })
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
    preferences: tauri::State<'_, Arc<preferences::Store>>,
) {
    let label = speaker.trim();
    if !label.is_empty()
        && let Err(error) =
            preferences.change(|saved| saved.sample_speaker = Some(label.to_string()))
    {
        tracing::warn!(%error, "the speaker label could not be remembered");
    }
    let recorded_with = RecordedWith {
        speaker,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        app_commit: env!("BUTTERVOICE_COMMIT").to_string(),
        chip: what_the_system_says("machdep.cpu.brand_string"),
        macos: what_the_system_says("kern.osproductversion"),
    };
    recorder.start(
        recorded_with,
        transcribers.dictation_language(),
        seconds_since_1970(),
    );
}

/// The label given last time, to start the field with.
#[tauri::command]
fn samples_speaker(preferences: tauri::State<'_, Arc<preferences::Store>>) -> String {
    preferences.get().sample_speaker.unwrap_or_default()
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
fn samples_next(
    said: String,
    recorder: tauri::State<'_, Arc<Recorder>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    recorder.next(&said).map_err(|error| {
        tracing::error!(%error, "a take could not be saved");
        Message::TakeNotSaved.text(transcribers.display_language())
    })
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
    recorder
        .finish()
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
            let _ = app.emit_to(MAIN_WINDOW, MODEL_DOWNLOAD, bytes);
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
async fn check_for_update(
    app: tauri::AppHandle,
    updates: tauri::State<'_, Updates>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<updates::Found, String> {
    let found = updates.check(&app).await.map_err(|error| {
        tracing::warn!(%error, "the update check failed");
        Message::UpdateCheckFailed.text(transcribers.display_language())
    })?;
    update_checked(&app);
    Ok(found)
}

/// Brings the menu and the main window up to date with the last check.
fn update_checked(app: &tauri::AppHandle) {
    refresh_menu(app);
    let known = app.state::<Updates>().known();
    let _ = app.emit_to(MAIN_WINDOW, UPDATE_KNOWN, known);
}

/// What the settings window shows without asking GitHub.
#[derive(serde::Serialize)]
struct UpdateSetup {
    daily: bool,
    known: Option<String>,
}

#[tauri::command]
fn update_setup(
    updates: tauri::State<'_, Updates>,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
) -> UpdateSetup {
    UpdateSetup {
        daily: preferences.get().check_for_updates_daily,
        known: updates.known(),
    }
}

#[tauri::command]
fn choose_daily_update_check(
    on: bool,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    preferences
        .change(|saved| saved.check_for_updates_daily = on)
        .map_err(|error| not_saved(error, &transcribers))
}

#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    updates: tauri::State<'_, Updates>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    updates.install(&app).await.map_err(|error| {
        tracing::error!(%error, "the update could not be installed");
        Message::UpdateNotInstalled.text(transcribers.display_language())
    })
}

#[tauri::command]
fn local_models(
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Vec<transcribers::LocalModel> {
    transcribers.local_models()
}

// `async`: choosing another model loads it.
#[tauri::command(async)]
fn choose_local_model(
    id: Option<String>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    transcribers
        .choose_local_model(id)
        .map_err(|error| not_saved(error, &transcribers))
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

#[tauri::command]
fn hotkey_setup(preferences: tauri::State<'_, Arc<preferences::Store>>) -> &'static str {
    preferences.get().hotkey.name()
}

/// The windows are told through an event so each names the new key.
const HOTKEY_CHANGED: &str = "hotkey";

#[tauri::command]
fn choose_hotkey(
    key: String,
    app: tauri::AppHandle,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    dictations: tauri::State<'_, Dictations>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    let key = HotkeyKey::from_setting(Some(&key));
    preferences
        .change(|saved| saved.hotkey = key)
        .map_err(|error| not_saved(error, &transcribers))?;
    lock(&dictations).set_key(key);
    if let Err(error) = app.emit(HOTKEY_CHANGED, key.name()) {
        tracing::warn!(%error, "the webviews could not be notified of the hotkey change");
    }
    Ok(())
}

/// Packs the set just saved into the Downloads folder and shows it in the
/// Finder. Sending it is the person's to do; the app sends nothing.
#[tauri::command(async)]
fn samples_package(
    app: tauri::AppHandle,
    recorder: tauri::State<'_, Arc<Recorder>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<String, String> {
    let not_made = |error: &dyn std::fmt::Display| {
        tracing::error!(%error, "the samples package could not be made");
        Message::PackageNotMade.text(transcribers.display_language())
    };
    let downloads = app.path().download_dir().map_err(|e| not_made(&e))?;
    let package = recorder
        .package_last_saved(&downloads)
        .map_err(|e| not_made(&e))?;
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&package)
        .status();
    Ok(package.display().to_string())
}

#[tauri::command(async)]
fn forget_api_key(transcribers: tauri::State<'_, Arc<Transcribers>>) -> Result<(), String> {
    transcribers.forget_key().map_err(|error| {
        tracing::warn!(%error, "the API key could not be removed");
        Message::KeyNotRemoved.text(transcribers.display_language())
    })
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

/// `update_found` adds an item that leads to installing it. Stop
/// Dictation can be chosen only while one is `capturing`.
fn menu_in<R: tauri::Runtime, M: Manager<R>>(
    app: &M,
    language: Language,
    update_found: bool,
    capturing: bool,
) -> tauri::Result<Menu<R>> {
    let item = |id, message: Message| {
        MenuItem::with_id(app, id, message.text(language), true, None::<&str>)
    };
    let stop = MenuItem::with_id(
        app,
        STOP,
        Message::StopDictation.text(language),
        capturing,
        None::<&str>,
    )?;
    let copy_last = item(COPY_LAST, Message::CopyLastDictation)?;
    let mark_bad = item(MARK_BAD, Message::MarkLastDictationAsBad)?;
    let record_samples = item(SAMPLES, Message::RecordSamples)?;
    let open = item(OPEN, Message::OpenButterVoice)?;
    let settings = item(SETTINGS, Message::OpenSettings)?;
    let quit = item(QUIT, Message::Quit)?;
    if update_found {
        let install = item(INSTALL_UPDATE, Message::InstallUpdate)?;
        Menu::with_items(
            app,
            &[
                &install,
                &open,
                &stop,
                &copy_last,
                &mark_bad,
                &record_samples,
                &settings,
                &quit,
            ],
        )
    } else {
        Menu::with_items(
            app,
            &[
                &open,
                &stop,
                &copy_last,
                &mark_bad,
                &record_samples,
                &settings,
                &quit,
            ],
        )
    }
}

/// Rebuilds the menu bar item's menu for the current language and update.
fn refresh_menu(app: &tauri::AppHandle) {
    let language = app.state::<Arc<Transcribers>>().display_language();
    let update_found = app.state::<Updates>().known().is_some();
    let capturing = capturing(lock(&app.state::<Dictations>()).status());
    if let Some(item) = app.tray_by_id(MENU_BAR_ITEM)
        && let Ok(menu) = menu_in(app, language, update_found, capturing)
    {
        let _ = item.set_menu(Some(menu));
    }
}

fn refresh_window_titles(app: &tauri::AppHandle, language: Language) {
    for (label, message) in [
        (MAIN_WINDOW, Message::MainTitle),
        (SAMPLES, Message::SamplesTitle),
    ] {
        if let Some(window) = app.get_webview_window(label)
            && let Err(error) = window.set_title(&message.text(language))
        {
            tracing::warn!(%error, label, "the window title could not be updated");
        }
    }
}

const INSTALL_UPDATE: &str = "install-update";

/// How often the app looks at whether a daily check is due, and how long
/// after starting it first looks: not at once, so starting stays quick.
const UPDATE_LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const UPDATE_FIRST_LOOK: std::time::Duration = std::time::Duration::from_secs(60);

fn check_for_updates_daily(app: tauri::AppHandle, preferences: Arc<preferences::Store>) {
    let looking = std::thread::Builder::new()
        .name("update-check".into())
        .spawn(move || {
            std::thread::sleep(UPDATE_FIRST_LOOK);
            loop {
                let saved = preferences.get();
                let now = seconds_since_1970();
                if check_due(saved.check_for_updates_daily, saved.last_update_check, now) {
                    // Recorded before the check, so a Mac that is offline
                    // tries once a day, not every hour.
                    let _ = preferences.change(|saved| saved.last_update_check = Some(now));
                    let updates = app.state::<Updates>();
                    match tauri::async_runtime::block_on(updates.check(&app)) {
                        Ok(_) => update_checked(&app),
                        Err(error) => tracing::warn!(%error, "the daily update check failed"),
                    }
                }
                std::thread::sleep(UPDATE_LOOK_EVERY);
            }
        });
    if let Err(error) = looking {
        tracing::error!(%error, "the daily update check could not be started");
    }
}

/// From this run, or from the history when it is kept.
#[tauri::command]
fn last_dictation(
    dictations: tauri::State<'_, Dictations>,
    history: tauri::State<'_, Arc<History>>,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
) -> Option<String> {
    let this_run = lock(&dictations).last_dictation_text().map(str::to_string);
    this_run.or_else(|| {
        preferences
            .get()
            .keep_history
            .then(|| history.search("", 1).pop().map(|entry| entry.text))
            .flatten()
    })
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), ()> {
    clipboard::copy(&text).then_some(()).ok_or(())
}

#[tauri::command]
fn history_setup(preferences: tauri::State<'_, Arc<preferences::Store>>) -> bool {
    preferences.get().keep_history
}

#[tauri::command]
fn choose_keep_history(
    on: bool,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    preferences
        .change(|saved| saved.keep_history = on)
        .map_err(|error| not_saved(error, &transcribers))
}

#[tauri::command]
fn filler_words_setup(preferences: tauri::State<'_, Arc<preferences::Store>>) -> bool {
    preferences.get().drop_filler_words
}

#[tauri::command]
fn choose_drop_filler_words(
    on: bool,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    preferences
        .change(|saved| saved.drop_filler_words = on)
        .map_err(|error| not_saved(error, &transcribers))
}

#[tauri::command]
fn history_search(
    query: String,
    history: tauri::State<'_, Arc<History>>,
) -> Vec<buttervoice_history::Entry> {
    history.search(&query, HISTORY_SHOWN)
}

#[tauri::command]
fn clear_history(
    history: tauri::State<'_, Arc<History>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    history.clear().map_err(|error| {
        tracing::error!(%error, "the history could not be cleared");
        Message::HistoryNotCleared.text(transcribers.display_language())
    })
}

/// The date on this Mac's calendar `days_ago`, such as `2026-11-22`.
fn local_date(days_ago: u64) -> String {
    let today = chrono::Local::now().date_naive();
    (today - chrono::Days::new(days_ago))
        .format("%Y-%m-%d")
        .to_string()
}

/// Every day with a dictation, for the heatmap.
#[tauri::command]
fn usage_days(
    usage: tauri::State<'_, Arc<Usage>>,
) -> std::collections::BTreeMap<String, usage::Day> {
    usage.days()
}

#[derive(serde::Serialize)]
struct TimeSaved {
    days: u64,
    words_per_minute: u32,
    dictations: u64,
    words: u64,
    spoken_ms: u64,
    saved_ms: u64,
}

#[tauri::command]
fn time_saved(
    usage: tauri::State<'_, Arc<Usage>>,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
) -> TimeSaved {
    let since = local_date(TIME_SAVED_DAYS - 1);
    let (dictations, words, spoken_ms) =
        usage
            .days()
            .range(since..)
            .fold((0, 0, 0), |(dictations, words, spoken), (_, day)| {
                (
                    dictations + day.dictations,
                    words + day.words,
                    spoken + day.spoken_ms,
                )
            });
    let words_per_minute = preferences.get().typing_words_per_minute;
    TimeSaved {
        days: TIME_SAVED_DAYS,
        words_per_minute,
        dictations,
        words,
        spoken_ms,
        saved_ms: usage::time_saved(words, spoken_ms, words_per_minute).as_millis() as u64,
    }
}

#[tauri::command]
fn typing_speed(preferences: tauri::State<'_, Arc<preferences::Store>>) -> u32 {
    preferences.get().typing_words_per_minute
}

#[tauri::command]
fn choose_typing_speed(
    words_per_minute: u32,
    preferences: tauri::State<'_, Arc<preferences::Store>>,
    transcribers: tauri::State<'_, Arc<Transcribers>>,
) -> Result<(), String> {
    preferences
        .change(|saved| saved.typing_words_per_minute = words_per_minute.max(1))
        .map_err(|error| not_saved(error, &transcribers))
}

fn seconds_since_1970() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

const MENU_BAR_ITEM: &str = "menu-bar";

fn add_menu_bar_item(app: &tauri::App, language: Language) -> tauri::Result<()> {
    let menu = menu_in(app, language, false, false)?;
    let mut item = TrayIconBuilder::with_id(MENU_BAR_ITEM)
        .tooltip("ButterVoice")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT {
                app.exit(0);
            } else if event.id() == STOP {
                let (command, status) = {
                    let state = app.state::<Dictations>();
                    let mut dictations = lock(&state);
                    (dictations.stop_by_hand(key_tap::now()), dictations.status())
                };
                if let Some(command) = command {
                    let _ = app.state::<CaptureCommands>().0.send(command);
                }
                app.state::<Arc<Hud>>().show(status);
                refresh_menu(app);
            } else if event.id() == COPY_LAST {
                // Only when asked: the clipboard is the person's own.
                let text = lock(&app.state::<Dictations>())
                    .last_dictation_text()
                    .map(str::to_string);
                if let Some(text) = text
                    && !clipboard::copy(&text)
                {
                    tracing::warn!("the last dictation could not be copied");
                }
            } else if event.id() == MARK_BAD {
                let latest = lock(&app.state::<Dictations>()).latest_dictation();
                buttervoice_telemetry::record_marked_bad(latest);
            } else if event.id() == OPEN {
                show_main_window(app, "home");
            } else if event.id() == SETTINGS {
                show_main_window(app, "dictation");
            } else if event.id() == INSTALL_UPDATE {
                // General shows the update and installs it.
                show_main_window(app, "general");
            } else if event.id() == SAMPLES
                && let Some(window) = app.get_webview_window(SAMPLES)
            {
                app.state::<Arc<Recorder>>().window_shown(true);
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    // A template image: macOS colours it to suit the menu bar. The canvas
    // is cropped to Ghee, because the menu bar item is drawn 18 pt tall.
    item = item
        .icon(Image::from_bytes(include_bytes!(
            "../icons/menu-bar/ghee-template.png"
        ))?)
        .icon_as_template(true);
    item.build(app)?;
    Ok(())
}

/// Not `settings.json`: the original app keeps a file of that name in the
/// same folder, in a different shape, and this one must not overwrite it.
const PREFERENCES_FILE: &str = "preferences.json";

/// A session only takes dictations while its window is showing. With the
/// window closed part-way through, dictation goes back to typing and the
/// session waits.
/// Both the menu item and the window it opens.
const SAMPLES: &str = "samples";
const SAMPLES_CHANGED: &str = "samples-changed";

const QUIT: &str = "quit";
const MARK_BAD: &str = "mark-bad";
const COPY_LAST: &str = "copy-last";
const STOP: &str = "stop-dictation";
const OPEN: &str = "open";
const SETTINGS: &str = "settings";
/// Home, the history and the settings sections, in one window.
const MAIN_WINDOW: &str = "main";
const SHOW_SECTION: &str = "show-section";
const UPDATE_KNOWN: &str = "update-known";
const DICTATION_FINISHED: &str = "dictation-finished";
const HISTORY_FILE: &str = "history.jsonl";
const USAGE_FILE: &str = "usage.json";
/// Time saved is shown for this many days up to today.
const TIME_SAVED_DAYS: u64 = 30;
const HISTORY_SHOWN: usize = 200;

fn show_main_window(app: &tauri::AppHandle, section: &str) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = app.emit_to(MAIN_WINDOW, SHOW_SECTION, section);
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The state behind the lock is only ever changed by whole method calls, so
/// it is still usable after another thread panicked while holding it.
fn lock(dictations: &Mutex<KeyDictations>) -> MutexGuard<'_, KeyDictations> {
    dictations.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Emptied when the app quits. Tauri ends the process without running
/// destructors, so nothing else would write out the last log lines.
struct RunningTelemetry(Mutex<Option<Telemetry>>);

// The bridge lets an assistant run JavaScript in the app's windows, so it
// must never reach the team.
#[cfg(all(feature = "mcp-bridge", not(debug_assertions)))]
compile_error!("the mcp-bridge feature is for development builds only");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    // Only this Mac may connect: by default the bridge listens on every
    // network.
    #[cfg(feature = "mcp-bridge")]
    let builder = builder.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .build(),
    );
    builder
        .manage(Updates::default())
        .setup(|app| {
            // Granted here rather than in the capabilities folder, so a
            // build without the bridge carries no trace of it.
            #[cfg(feature = "mcp-bridge")]
            app.add_capability(
                tauri::ipc::CapabilityBuilder::new("mcp-bridge")
                    .windows(["hud", MAIN_WINDOW, SAMPLES])
                    .permission("mcp-bridge:default"),
            )?;
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
            let language = transcribers.display_language();
            refresh_window_titles(app.handle(), language);
            add_menu_bar_item(app, language)?;
            let hud = Arc::new(Hud::set_up(app.handle())?);
            let dictations: Dictations = Arc::new(Mutex::new(KeyDictations::new(
                preferences.get().hotkey,
                Mode::PushToTalk,
            )));
            app.manage(dictations.clone());
            let recorder = Arc::new(Recorder::new(app.path().app_data_dir()?.join("samples")));
            let history = Arc::new(History::open(app.path().app_data_dir()?.join(HISTORY_FILE)));
            app.manage(history.clone());
            let usage = Arc::new(Usage::open(app.path().app_data_dir()?.join(USAGE_FILE)));
            app.manage(usage.clone());
            app.manage(recorder.clone());
            app.manage(hud.clone());
            let commands = listen_for_the_dictation_key(
                dictations,
                hud,
                transcribers,
                recorder,
                preferences.clone(),
                Kept { history, usage },
                app.handle().clone(),
            )?;
            app.manage(commands);
            check_for_updates_daily(app.handle().clone(), preferences.clone());
            app.manage(preferences);
            Ok(())
        })
        // Closing a window only hides it, so it can be opened again.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && (window.label() == MAIN_WINDOW || window.label() == SAMPLES)
            {
                api.prevent_close();
                if window.label() == SAMPLES {
                    window.state::<Arc<Recorder>>().window_shown(false);
                }
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            transcription_setup,
            language_setup,
            microphone_setup,
            download_model,
            local_models,
            choose_local_model,
            check_for_update,
            update_setup,
            choose_daily_update_check,
            install_update,
            cancel_model_download,
            choose_microphone,
            hotkey_setup,
            choose_hotkey,
            samples_state,
            samples_start,
            samples_speaker,
            samples_next,
            samples_skip,
            samples_back,
            samples_audio,
            samples_cancel,
            samples_finish,
            samples_package,
            choose_language,
            choose_provider,
            save_api_key,
            forget_api_key,
            last_dictation,
            copy_text,
            history_setup,
            choose_keep_history,
            filler_words_setup,
            choose_drop_filler_words,
            history_search,
            clear_history,
            usage_days,
            time_saved,
            typing_speed,
            choose_typing_speed
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
