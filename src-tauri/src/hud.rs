//! The small always-on-top window that shows what a dictation is doing. It
//! stays on screen the whole time and fades itself in and out, so nothing has
//! to be shown or hidden while another app has the keyboard.

use std::sync::{Mutex, PoisonError};

use buttervoice_core::key_dictation::DictationStatus;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

const WINDOW: &str = "hud";
const STATUS_EVENT: &str = "dictation-status";
/// In points, from the bottom edge of the main screen.
const BOTTOM_MARGIN: f64 = 96.0;

pub struct Hud {
    app: AppHandle,
    shown: Mutex<DictationStatus>,
}

impl Hud {
    pub fn set_up(app: &AppHandle) -> tauri::Result<Self> {
        if let Some(window) = app.get_webview_window(WINDOW) {
            // Clicks go through to whatever is underneath.
            window.set_ignore_cursor_events(true)?;
            if let Some(screen) = window.primary_monitor()? {
                let window_size = window.outer_size()?;
                let margin = (BOTTOM_MARGIN * screen.scale_factor()) as i32;
                window.set_position(PhysicalPosition::new(
                    screen.position().x
                        + (screen.size().width as i32 - window_size.width as i32) / 2,
                    screen.position().y + screen.size().height as i32
                        - window_size.height as i32
                        - margin,
                ))?;
            }
        }
        Ok(Self {
            app: app.clone(),
            shown: Mutex::new(DictationStatus::Idle),
        })
    }

    pub fn show(&self, status: DictationStatus) {
        let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        if *shown == status {
            return;
        }
        *shown = status;
        let name = match status {
            DictationStatus::Idle => "idle",
            DictationStatus::Starting => "starting",
            DictationStatus::Listening => "listening",
            DictationStatus::Working => "working",
        };
        self.emit(name);
    }

    /// Says for a moment that a dictation could not be typed. The window
    /// fades it out itself, or the next dictation replaces it.
    pub fn not_typed(&self) {
        self.emit("not_typed");
    }

    fn emit(&self, name: &str) {
        if let Err(error) = self.app.emit_to(WINDOW, STATUS_EVENT, name) {
            tracing::warn!(%error, "the HUD could not be updated");
        }
    }
}
