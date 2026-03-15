use tauri::{
    menu::{Menu, MenuBuilder},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt;

use crate::app_state::AppState;
use crate::settings::SettingsPatch;

const MENU_SETTINGS: &str = "settings";
const MENU_TOGGLE_LOGIN: &str = "toggle_launch_at_login";
const MENU_DEBUG_START: &str = "debug_start_dictation";
const MENU_QUIT: &str = "quit";
pub const TRAY_ID: &str = "main";

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    launch_at_login: bool,
) -> tauri::Result<Menu<R>> {
    let login_label = if launch_at_login {
        "Disable Launch at Login"
    } else {
        "Enable Launch at Login"
    };

    MenuBuilder::new(app)
        .text(MENU_SETTINGS, "Settings")
        .text(MENU_TOGGLE_LOGIN, login_label)
        .separator()
        .text(MENU_DEBUG_START, "Start Dictation (Debug)")
        .separator()
        .text(MENU_QUIT, "Quit")
        .build()
}

pub fn build_tray<R: Runtime>(app: &AppHandle<R>, launch_at_login: bool) -> tauri::Result<()> {
    let menu = build_tray_menu(app, launch_at_login)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu))?;
    }

    Ok(())
}

pub(crate) fn on_tray_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        MENU_SETTINGS => {
            let _ = show_settings_window(app);
        }
        MENU_TOGGLE_LOGIN => {
            let state = app.state::<std::sync::Arc<AppState>>();
            let settings = state.settings_store().get();
            let next_enabled = !settings.launch_at_login;

            let next = state
                .settings_store()
                .update(SettingsPatch {
                    launch_at_login: Some(next_enabled),
                    ..SettingsPatch::default()
                })
                .map_err(|err| eprintln!("failed toggling launch_at_login: {err:#}"));

            if next.is_ok() {
                let autolaunch = app.autolaunch();
                let result = if next_enabled {
                    autolaunch.enable()
                } else {
                    autolaunch.disable()
                };
                if let Err(err) = result {
                    eprintln!("failed applying launch_at_login via autostart plugin: {err:#}");
                }
            }

            let _ = refresh_login_menu_label(app);
        }
        MENU_DEBUG_START => {
            let state = app.state::<std::sync::Arc<AppState>>().inner().clone();
            state.start_recording();
        }
        MENU_QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}

pub fn show_settings_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    crate::dock_macos::set_dock_icon_visible(app, true);

    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
    }

    Ok(())
}

pub fn refresh_login_menu_label<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let settings = app
        .state::<std::sync::Arc<AppState>>()
        .settings_store()
        .get();

    let menu = build_tray_menu(app, settings.launch_at_login)?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu))?;
    }

    Ok(())
}
