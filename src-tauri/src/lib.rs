mod app_state;
mod audio;
mod content_classification;
mod dock_macos;
mod hotkey_macos;
mod llm_cleanup;
mod llm_guard;
mod models;
mod permissions_macos;
mod persona;
pub mod post_process;
mod remote_speech;
mod secrets;
pub mod settings;
mod speech_backend;
mod speech_preprocess;
mod telemetry;
mod text_inject_macos;
mod transcript_filter;
mod tray;
mod usage_stats;
mod whisper_backend;

use std::sync::Arc;
use std::time::Duration;

use app_state::{
    AppState, DebugLogEntry, DictationState, PipelineMetricsSnapshot, TranscriptLogEntry,
};
use audio::MicDevice;
use hotkey_macos::{DictationMode, HotkeyPresetInfo};
use llm_cleanup::LlmModelEntry;
use models::ModelInfo;
use permissions_macos::{PermissionKind, PermissionsStatus};
use remote_speech::SpeechModelEntry;
use settings::{Persona, Settings, SettingsPatch};
use tauri::{Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use usage_stats::DailyStat;
use whisper_backend::BackendStatus;

#[tauri::command]
fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.settings_store().get())
}

#[tauri::command]
fn update_settings(
    app_handle: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    patch: SettingsPatch,
) -> Result<Settings, String> {
    if let Some(ref hotkey) = patch.hotkey {
        hotkey_macos::validate_hotkey(hotkey).map_err(|err| err.to_string())?;
    }

    let previous = state.settings_store().get();
    let next = state
        .settings_store()
        .update(patch)
        .map_err(|err| err.to_string())?;

    state
        .inner()
        .clone()
        .apply_runtime_settings(&previous, &next);

    if previous.launch_at_login != next.launch_at_login {
        let autolaunch = app_handle.autolaunch();
        let result = if next.launch_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        result
            .map_err(|err| format!("settings saved, but launch-at-login update failed: {err}"))?;
    }

    Ok(next)
}

#[tauri::command]
fn list_microphones() -> Result<Vec<MicDevice>, String> {
    audio::AudioCapture::list_input_devices().map_err(|err| err.to_string())
}

#[tauri::command]
fn list_models() -> Vec<ModelInfo> {
    models::available_models()
}

#[tauri::command]
fn download_model(state: State<'_, Arc<AppState>>, model_id: String) -> Result<(), String> {
    state
        .backend()
        .download_model(&model_id)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn start_model_download(state: State<'_, Arc<AppState>>, model_id: String) -> Result<(), String> {
    state.start_model_download(model_id)
}

#[tauri::command]
fn cancel_model_download(state: State<'_, Arc<AppState>>, model_id: String) -> Result<(), String> {
    state.cancel_model_download(&model_id)
}

#[tauri::command]
fn list_downloaded_models(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    state
        .backend()
        .list_downloaded_models()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn delete_model(state: State<'_, Arc<AppState>>, model_id: String) -> Result<(), String> {
    state
        .backend()
        .delete_model(&model_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_backend_status(state: State<'_, Arc<AppState>>) -> Result<BackendStatus, String> {
    Ok(state.backend_status())
}

#[tauri::command]
fn list_remote_speech_models(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SpeechModelEntry>, String> {
    let settings = state
        .settings_with_resolved_speech_api_key(&state.settings_store().get())
        .map_err(|err| err.to_string())?;
    state
        .backend()
        .list_remote_models(&settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn test_remote_speech_connection(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let settings = state
        .settings_with_resolved_speech_api_key(&state.settings_store().get())
        .map_err(|err| err.to_string())?;
    state
        .backend()
        .test_remote_connection(&settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_permissions_status() -> PermissionsStatus {
    permissions_macos::get_permissions_status()
}

#[tauri::command]
fn request_permission(app_handle: tauri::AppHandle, kind: PermissionKind) -> Result<(), String> {
    permissions_macos::request_permission(&app_handle, kind)
}

#[tauri::command]
fn restart_app(app_handle: tauri::AppHandle) {
    app_handle.restart();
}

#[tauri::command]
fn get_transcript_logs(state: State<'_, Arc<AppState>>) -> Vec<TranscriptLogEntry> {
    state.get_transcript_logs()
}

#[tauri::command]
fn clear_transcript_logs(state: State<'_, Arc<AppState>>) {
    state.clear_transcript_logs();
}

#[tauri::command]
fn clear_transcript_logs_and_recordings(state: State<'_, Arc<AppState>>) {
    state.clear_transcript_logs_and_recordings();
}

#[tauri::command]
fn get_recording_audio(
    state: State<'_, Arc<AppState>>,
    filename: String,
) -> Result<Vec<u8>, String> {
    state
        .get_recording_audio(&filename)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_debug_logs(state: State<'_, Arc<AppState>>) -> Vec<DebugLogEntry> {
    state.get_debug_logs()
}

#[tauri::command]
fn clear_debug_logs(state: State<'_, Arc<AppState>>) {
    state.clear_debug_logs();
}

#[tauri::command]
fn get_dictation_state(state: State<'_, Arc<AppState>>) -> DictationState {
    state.get_dictation_state()
}

#[tauri::command]
fn get_audio_input_level(state: State<'_, Arc<AppState>>) -> u8 {
    state.audio_input_level_percent()
}

#[tauri::command]
fn debug_start_dictation(state: State<'_, Arc<AppState>>) {
    state.start_recording();
}

#[tauri::command]
fn debug_stop_dictation(state: State<'_, Arc<AppState>>) {
    state.inner().clone().stop_and_transcribe();
}

#[tauri::command]
fn get_default_enhancement_prompt() -> String {
    llm_cleanup::default_enhancement_prompt().to_string()
}

#[tauri::command]
fn list_hotkey_presets() -> Vec<HotkeyPresetInfo> {
    hotkey_macos::list_presets()
}

#[tauri::command]
fn list_llm_models(state: State<'_, Arc<AppState>>) -> Result<Vec<LlmModelEntry>, String> {
    let mut settings = state.settings_store().get();
    if !settings.beta_ai_enhancement_enabled {
        return Err("AI Enhancement beta is disabled in Advanced settings.".to_string());
    }
    if settings.llm_cleanup_base_url.trim().is_empty() {
        return Err("LLM base URL is empty. Set a base URL first.".to_string());
    }
    settings.llm_cleanup_api_key = secrets::load_llm_api_key()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    llm_cleanup::list_models(
        &settings.llm_cleanup_base_url,
        &settings.llm_cleanup_api_key,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn test_otlp_connection(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let settings = state.settings_store().get();
    telemetry::test_connection(&settings.telemetry_otlp_endpoint)
}

#[tauri::command]
fn test_llm_cleanup_connection(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let mut settings = state.settings_store().get();
    if !settings.beta_ai_enhancement_enabled {
        return Err("AI Enhancement beta is disabled in Advanced settings.".to_string());
    }
    if settings.llm_cleanup_model.trim().is_empty() {
        return Err("LLM model is empty. Set a model first.".to_string());
    }
    if settings.llm_cleanup_base_url.trim().is_empty() {
        return Err("LLM base URL is empty. Set a base URL first.".to_string());
    }
    settings.llm_cleanup_api_key = secrets::load_llm_api_key()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let sample_text = "this is a connection test for transcription cleanup";
    llm_cleanup::cleanup_text(&settings, sample_text).map_err(|err| err.to_string())
}

#[tauri::command]
fn enhance_text(state: State<'_, Arc<AppState>>, text: String) -> Result<String, String> {
    let mut settings = state.settings_store().get();
    if !settings.beta_ai_enhancement_enabled {
        return Err("AI Enhancement beta is disabled in Advanced settings.".to_string());
    }
    if settings.llm_cleanup_model.trim().is_empty() {
        return Err(
            "LLM model is not configured. Set a model in AI Enhancement settings.".to_string(),
        );
    }
    if settings.llm_cleanup_base_url.trim().is_empty() {
        return Err(
            "LLM base URL is not configured. Set a base URL in AI Enhancement settings."
                .to_string(),
        );
    }
    settings.llm_cleanup_api_key = secrets::load_llm_api_key()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    llm_cleanup::cleanup_text(&settings, &text).map_err(|err| err.to_string())
}

#[tauri::command]
fn test_post_process(
    state: State<'_, Arc<AppState>>,
    text: String,
) -> Result<post_process::PipelineResult, String> {
    state.test_post_process(&text).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_post_process_dictionary(state: State<'_, Arc<AppState>>) -> Vec<String> {
    state.settings_store().get().post_process_custom_dictionary
}

#[tauri::command]
fn update_post_process_dictionary(
    state: State<'_, Arc<AppState>>,
    words: Vec<String>,
) -> Result<(), String> {
    // Persist first so runtime and stored settings never diverge on error.
    let patch = SettingsPatch {
        post_process_custom_dictionary: Some(words.clone()),
        ..Default::default()
    };
    state
        .settings_store()
        .update(patch)
        .map_err(|e| e.to_string())?;
    state.update_post_process_dictionary(&words);
    Ok(())
}

#[tauri::command]
fn get_pipeline_metrics(state: State<'_, Arc<AppState>>) -> PipelineMetricsSnapshot {
    state.get_pipeline_metrics()
}

#[tauri::command]
fn clear_pipeline_metrics(state: State<'_, Arc<AppState>>) {
    state.clear_pipeline_metrics();
}

#[tauri::command]
fn purge_local_artifacts(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.purge_local_artifacts().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_usage_stats(state: State<'_, Arc<AppState>>) -> Vec<DailyStat> {
    state.usage_stats.get_stats()
}

#[tauri::command]
fn clear_usage_stats(state: State<'_, Arc<AppState>>) {
    state.usage_stats.clear_stats();
}

#[tauri::command]
fn get_personas(state: State<'_, Arc<AppState>>) -> Vec<Persona> {
    state.settings_store().get().personas
}

#[tauri::command]
fn add_persona(state: State<'_, Arc<AppState>>, persona: Persona) -> Result<Settings, String> {
    let current = state.settings_store().get();
    let mut personas = current.personas;
    personas.push(persona);
    state
        .settings_store()
        .update(SettingsPatch {
            personas: Some(personas),
            ..Default::default()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_persona(state: State<'_, Arc<AppState>>, persona: Persona) -> Result<Settings, String> {
    let current = state.settings_store().get();
    let mut personas = current.personas;
    if let Some(existing) = personas.iter_mut().find(|p| p.id == persona.id) {
        existing.name = persona.name;
        existing.system_prompt = persona.system_prompt;
        existing.model_override = persona.model_override;
    } else {
        return Err(format!("persona not found: {}", persona.id));
    }
    state
        .settings_store()
        .update(SettingsPatch {
            personas: Some(personas),
            ..Default::default()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_persona(state: State<'_, Arc<AppState>>, persona_id: String) -> Result<Settings, String> {
    let current = state.settings_store().get();
    if current
        .personas
        .iter()
        .any(|p| p.id == persona_id && p.is_default)
    {
        return Err("cannot delete a built-in persona".to_string());
    }
    let personas: Vec<_> = current
        .personas
        .into_iter()
        .filter(|p| p.id != persona_id)
        .collect();
    state
        .settings_store()
        .update(SettingsPatch {
            personas: Some(personas),
            ..Default::default()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn transform_with_persona(
    state: State<'_, Arc<AppState>>,
    text: String,
    persona_id: String,
) -> Result<String, String> {
    if !state.settings_store().get().beta_personas_enabled {
        return Err("Personas beta is disabled in Advanced settings.".to_string());
    }
    state
        .transform_with_persona(&text, &persona_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_default_classification_prompt() -> String {
    content_classification::default_classification_prompt().to_string()
}

#[tauri::command]
fn get_default_persona_prompt(persona_id: String) -> Result<String, String> {
    match persona_id.as_str() {
        settings::DEFAULT_PERSONA_PROFESSIONAL_ID => {
            Ok(persona::DEFAULT_PROFESSIONAL_TONE_PROMPT.to_string())
        }
        settings::DEFAULT_PERSONA_PROMPT_ENGINEER_ID => {
            Ok(persona::DEFAULT_PROMPT_ENGINEER_PROMPT.to_string())
        }
        _ => Err(format!("no default prompt for persona: {persona_id}")),
    }
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            list_microphones,
            list_models,
            download_model,
            start_model_download,
            cancel_model_download,
            list_downloaded_models,
            delete_model,
            get_backend_status,
            list_remote_speech_models,
            test_remote_speech_connection,
            get_permissions_status,
            request_permission,
            restart_app,
            get_transcript_logs,
            clear_transcript_logs,
            clear_transcript_logs_and_recordings,
            get_recording_audio,
            get_debug_logs,
            clear_debug_logs,
            get_dictation_state,
            get_audio_input_level,
            debug_start_dictation,
            debug_stop_dictation,
            list_llm_models,
            test_llm_cleanup_connection,
            enhance_text,
            get_default_enhancement_prompt,
            list_hotkey_presets,
            test_post_process,
            get_post_process_dictionary,
            update_post_process_dictionary,
            get_pipeline_metrics,
            clear_pipeline_metrics,
            purge_local_artifacts,
            get_usage_stats,
            clear_usage_stats,
            get_personas,
            add_persona,
            update_persona,
            delete_persona,
            transform_with_persona,
            get_default_classification_prompt,
            get_default_persona_prompt,
            test_otlp_connection,
        ])
        .on_menu_event(|app, event| tray::on_tray_menu_event(app, event.id().as_ref()))
        .on_window_event(handle_window_event)
        .setup(|app| {
            let state = AppState::bootstrap(app.handle().clone()).map_err(
                |err| -> Box<dyn std::error::Error> {
                    Box::new(std::io::Error::other(err.to_string()))
                },
            )?;

            // Initialize OpenTelemetry (before any tracing calls).
            let settings = state.settings_store().get();
            telemetry::init(
                settings.telemetry_enabled,
                &settings.telemetry_otlp_endpoint,
            );

            state.preflight_permissions();
            app.manage(state.clone());

            let launch_at_login = state.settings_store().get().launch_at_login;
            tray::build_tray(app.handle(), launch_at_login)?;
            state.clone().schedule_startup_warmup();

            tray::refresh_login_menu_label(app.handle())?;
            tray::show_settings_window(app.handle())?;
            configure_hud_window(app.handle());

            let hotkey_config = state.hotkey_config();
            spawn_hotkey_listener_with_retry(state.clone(), hotkey_config);
            spawn_mic_watcher(state.clone());

            Ok(())
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running ButterVoice application");
}

fn spawn_hotkey_listener_with_retry(
    state: Arc<AppState>,
    config: &'static hotkey_macos::HotkeyConfig,
) {
    std::thread::spawn(move || loop {
        let hotkey_state = state.clone();
        match hotkey_macos::spawn_hotkey_listener(config, move |event| {
            let mode = config.dictation_mode();

            match mode {
                DictationMode::PushToTalk => match event {
                    hotkey_macos::HotkeyEvent::Pressed => {
                        eprintln!("hotkey event: pressed (push-to-talk)");
                        hotkey_state.start_recording();
                    }
                    hotkey_macos::HotkeyEvent::Released => {
                        eprintln!("hotkey event: released (push-to-talk)");
                        hotkey_state.clone().stop_and_transcribe();
                    }
                },
                DictationMode::Toggle => match event {
                    hotkey_macos::HotkeyEvent::Pressed => {
                        let current = hotkey_state.get_dictation_state();
                        if matches!(current, DictationState::Recording) {
                            eprintln!("hotkey event: pressed (toggle → stop)");
                            hotkey_state.clone().stop_and_transcribe();
                        } else if matches!(current, DictationState::Idle | DictationState::Error) {
                            eprintln!("hotkey event: pressed (toggle → start)");
                            hotkey_state.start_recording();
                        }
                    }
                    hotkey_macos::HotkeyEvent::Released => {
                        // In toggle mode, release is a no-op
                    }
                },
            }
        }) {
            Ok(()) => {
                eprintln!("hotkey listener started");
                return;
            }
            Err(err) => {
                eprintln!("hotkey listener unavailable: {err:#}");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    });
}

fn spawn_mic_watcher(state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(3));
        state.reconcile_mic_if_stale();
    });
}

fn configure_hud_window(app: &tauri::AppHandle) {
    let Some(hud) = app.get_webview_window("hud") else {
        return;
    };
    // The HUD is purely visual — let all mouse events pass through at the
    // OS level. CSS pointer-events does not affect OS hit testing on a
    // transparent window.
    if let Err(err) = hud.set_ignore_cursor_events(true) {
        eprintln!("failed setting hud ignore-cursor-events: {err}");
    }
    // Re-assert at runtime: the tauri.conf.json flag is not always applied
    // reliably at window creation.
    if let Err(err) = hud.set_visible_on_all_workspaces(true) {
        eprintln!("failed setting hud visible-on-all-workspaces: {err}");
    }
    #[cfg(target_os = "macos")]
    configure_hud_collection_behavior(&hud);
}

/// Allow the HUD to follow the active Space, including Spaces owned by
/// fullscreen apps. Tauri's `visibleOnAllWorkspaces` only sets
/// CanJoinAllSpaces; without FullScreenAuxiliary the window never appears
/// over fullscreen applications. Must run after
/// `set_visible_on_all_workspaces`, which rewrites the collection behavior.
#[cfg(target_os = "macos")]
#[allow(deprecated)] // cocoa is the crate already in use; objc2 migration is separate work
fn configure_hud_collection_behavior(hud: &tauri::WebviewWindow) {
    use cocoa::appkit::{NSWindow, NSWindowCollectionBehavior};

    let Ok(ns_window) = hud.ns_window() else {
        return;
    };
    let ns_window = ns_window as cocoa::base::id;
    unsafe {
        let behavior = ns_window.collectionBehavior()
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary;
        ns_window.setCollectionBehavior_(behavior);
    }
}

fn handle_window_event<R: tauri::Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        match window.label() {
            "settings" => {
                api.prevent_close();
                let app_handle = window.app_handle();
                let _ = window.hide();
                dock_macos::set_dock_icon_visible(app_handle, false);
            }
            "hud" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        }
    }
}
