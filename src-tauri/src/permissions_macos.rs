#[cfg(target_os = "macos")]
use cpal::traits::StreamTrait;
use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use std::process::Command;
#[cfg(target_os = "macos")]
use std::time::Duration;
use tauri::AppHandle;

use crate::hotkey_macos;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsStatus {
    pub microphone: PermissionState,
    pub accessibility: PermissionState,
    pub input_monitoring: PermissionState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Microphone,
    Accessibility,
    InputMonitoring,
}

pub fn get_permissions_status() -> PermissionsStatus {
    #[cfg(target_os = "macos")]
    {
        PermissionsStatus {
            microphone: microphone_permission_state(),
            accessibility: accessibility_permission_state(),
            input_monitoring: input_monitoring_permission_state(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        PermissionsStatus {
            microphone: PermissionState::Unknown,
            accessibility: PermissionState::Unknown,
            input_monitoring: PermissionState::Unknown,
        }
    }
}

pub fn request_permission(_app: &AppHandle, kind: PermissionKind) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        match kind {
            PermissionKind::Microphone => {
                // Attempting to start a short-lived input stream triggers the
                // native microphone permission flow and helps ensure a TCC row
                // is created for the app on first run.
                let _ = request_microphone_access();
            }
            PermissionKind::Accessibility => {
                // Prompt-capable counterpart to CGPreflightPostEventAccess.
                let _ = unsafe { CGRequestPostEventAccess() };
            }
            PermissionKind::InputMonitoring => {
                let _ = hotkey_macos::request_global_input_access();
            }
        }
    }

    #[cfg(target_os = "macos")]
    if matches!(kind, PermissionKind::InputMonitoring) && hotkey_macos::has_global_input_access() {
        return Ok(());
    }

    let url = match kind {
        PermissionKind::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        PermissionKind::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
    };

    let status = Command::new("open")
        .arg(url)
        .status()
        .map_err(|err| err.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open permissions URL: {}", status))
    }
}

pub fn ensure_preflight_status(_app: &AppHandle) {
    let status = get_permissions_status();
    #[cfg(target_os = "macos")]
    if !matches!(status.input_monitoring, PermissionState::Granted) {
        let _ = hotkey_macos::request_global_input_access();
    }
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
}

#[cfg(target_os = "macos")]
fn accessibility_permission_state() -> PermissionState {
    let has_ax_trust = unsafe { AXIsProcessTrusted() != 0 };
    let has_post_event_access = unsafe { CGPreflightPostEventAccess() };
    if has_ax_trust || has_post_event_access {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

#[cfg(target_os = "macos")]
fn input_monitoring_permission_state() -> PermissionState {
    if hotkey_macos::has_global_input_access() {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

#[cfg(target_os = "macos")]
fn microphone_permission_state() -> PermissionState {
    if can_open_microphone_stream(false) {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_access() -> bool {
    can_open_microphone_stream(true)
}

#[cfg(target_os = "macos")]
fn can_open_microphone_stream(start_stream: bool) -> bool {
    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        return false;
    };

    let supported_config = match device.default_input_config() {
        Ok(config) => config,
        Err(_) => return false,
    };
    let stream_config = supported_config.config();

    let stream_result = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => {
            device.build_input_stream(&stream_config, |_data: &[f32], _| {}, |_err| {}, None)
        }
        cpal::SampleFormat::I16 => {
            device.build_input_stream(&stream_config, |_data: &[i16], _| {}, |_err| {}, None)
        }
        cpal::SampleFormat::U16 => {
            device.build_input_stream(&stream_config, |_data: &[u16], _| {}, |_err| {}, None)
        }
        _ => return false,
    };

    let Ok(stream) = stream_result else {
        return false;
    };

    if start_stream {
        if stream.play().is_err() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    true
}
