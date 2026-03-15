use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

// ── Data model ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyKey {
    #[default]
    RightOption,
    LeftOption,
    RightCommand,
    RightControl,
    LeftControl,
    Fn,
    #[serde(untagged)]
    Custom {
        keycode: i64,
        is_modifier: bool,
    },
}

impl HotkeyKey {
    pub fn all_presets() -> &'static [HotkeyKey] {
        &[
            HotkeyKey::RightOption,
            HotkeyKey::LeftOption,
            HotkeyKey::RightCommand,
            HotkeyKey::RightControl,
            HotkeyKey::LeftControl,
            HotkeyKey::Fn,
        ]
    }

    pub fn spec(self) -> HotkeySpec {
        match self {
            Self::RightOption => HotkeySpec {
                keycode: 61,
                is_modifier: true,
                flag_mask: 0x0008_0000,
                display_label: "Right Option (\u{2325})",
            },
            Self::LeftOption => HotkeySpec {
                keycode: 58,
                is_modifier: true,
                flag_mask: 0x0008_0000,
                display_label: "Left Option (\u{2325})",
            },
            Self::RightCommand => HotkeySpec {
                keycode: 54,
                is_modifier: true,
                flag_mask: 0x0010_0000,
                display_label: "Right Command (\u{2318})",
            },
            Self::RightControl => HotkeySpec {
                keycode: 62,
                is_modifier: true,
                flag_mask: 0x0004_0000,
                display_label: "Right Control (\u{2303})",
            },
            Self::LeftControl => HotkeySpec {
                keycode: 59,
                is_modifier: true,
                flag_mask: 0x0004_0000,
                display_label: "Left Control (\u{2303})",
            },
            Self::Fn => HotkeySpec {
                keycode: 63,
                is_modifier: true,
                flag_mask: 0x0080_0000,
                display_label: "Fn (Globe)",
            },
            Self::Custom {
                keycode,
                is_modifier,
            } => HotkeySpec {
                keycode,
                is_modifier,
                flag_mask: 0,
                display_label: "Custom",
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct HotkeySpec {
    pub keycode: i64,
    pub is_modifier: bool,
    pub flag_mask: u64,
    pub display_label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DictationMode {
    #[default]
    PushToTalk,
    Toggle,
}

// ── Shared atomic config read by the CGEventTap callback ────────────────────

pub struct HotkeyConfig {
    keycode: AtomicI64,
    is_modifier: AtomicBool,
    flag_mask: AtomicU64,
    dictation_mode: AtomicU8,
}

impl HotkeyConfig {
    pub fn new(key: &HotkeyKey, mode: DictationMode) -> Self {
        let spec = key.spec();
        Self {
            keycode: AtomicI64::new(spec.keycode),
            is_modifier: AtomicBool::new(spec.is_modifier),
            flag_mask: AtomicU64::new(spec.flag_mask),
            dictation_mode: AtomicU8::new(mode as u8),
        }
    }

    pub fn update(&self, key: &HotkeyKey, mode: DictationMode) {
        let spec = key.spec();
        self.keycode.store(spec.keycode, Ordering::Release);
        self.is_modifier.store(spec.is_modifier, Ordering::Release);
        self.flag_mask.store(spec.flag_mask, Ordering::Release);
        self.dictation_mode.store(mode as u8, Ordering::Release);
    }

    pub fn dictation_mode(&self) -> DictationMode {
        match self.dictation_mode.load(Ordering::Acquire) {
            1 => DictationMode::Toggle,
            _ => DictationMode::PushToTalk,
        }
    }
}

// ── Validation ──────────────────────────────────────────────────────────────

pub fn validate_hotkey(key: &HotkeyKey) -> Result<()> {
    if let HotkeyKey::Custom { keycode, .. } = key {
        if *keycode < 0 || *keycode > 127 {
            return Err(anyhow!("invalid keycode {keycode} (must be 0–127)"));
        }
    }
    Ok(())
}

// ── Preset info for frontend ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HotkeyPresetInfo {
    pub key: HotkeyKey,
    pub label: String,
    pub description: String,
}

pub fn list_presets() -> Vec<HotkeyPresetInfo> {
    HotkeyKey::all_presets()
        .iter()
        .map(|key| {
            let spec = key.spec();
            HotkeyPresetInfo {
                key: *key,
                label: spec.display_label.to_string(),
                description: format!("macOS keycode {}", spec.keycode),
            }
        })
        .collect()
}

// ── macOS implementation ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::{HotkeyConfig, HotkeyEvent};
    use anyhow::{anyhow, Result};
    use crossbeam_channel::{bounded, unbounded, Sender};
    use std::ffi::c_void;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    type CGEventTapProxy = *mut c_void;
    type CGEventRef = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFAllocatorRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFIndex = isize;
    type CGEventType = u32;
    type CGEventMask = u64;

    type CGEventTapCallBack = extern "C" fn(
        proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef;

    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

    const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
    const K_CG_EVENT_KEY_UP: CGEventType = 11;
    const K_CG_EVENT_FLAGS_CHANGED: CGEventType = 12;

    const K_CG_KEYBOARD_EVENT_KEYCODE: i32 = 9;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;

        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: CGEventMask,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;

        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

        fn CGEventGetIntegerValueField(event: CGEventRef, field: i32) -> i64;

        fn CGEventGetFlags(event: CGEventRef) -> u64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: CFIndex,
        ) -> CFRunLoopSourceRef;

        fn CFRunLoopGetCurrent() -> CFRunLoopRef;

        fn CFRunLoopAddSource(
            run_loop: CFRunLoopRef,
            source: CFRunLoopSourceRef,
            mode: CFStringRef,
        );

        fn CFRunLoopRun();

        fn CFRelease(value: *const c_void);

        static kCFRunLoopCommonModes: CFStringRef;
    }

    struct CallbackContext {
        event_tx: Sender<HotkeyEvent>,
        config: &'static HotkeyConfig,
    }

    pub fn spawn_hotkey_listener<F>(config: &'static HotkeyConfig, callback: F) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        let (event_tx, event_rx) = unbounded::<HotkeyEvent>();
        std::thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                callback(event);
            }
        });

        let (ready_tx, ready_rx) = bounded::<std::result::Result<(), String>>(1);
        std::thread::spawn(move || {
            let ctx = Box::new(CallbackContext { event_tx, config });
            let user_info = Box::into_raw(ctx) as *mut c_void;

            let tap = unsafe {
                CGEventTapCreate(
                    K_CG_HID_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
                    event_mask_all(),
                    event_tap_callback,
                    user_info,
                )
            };

            if tap.is_null() {
                let _ = ready_tx.send(Err(
                    "failed to create global event tap (check Input Monitoring permission)"
                        .to_string(),
                ));
                unsafe {
                    drop(Box::from_raw(user_info as *mut CallbackContext));
                }
                return;
            }

            let source = unsafe { CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0) };
            if source.is_null() {
                let _ =
                    ready_tx.send(Err("failed to create event tap run loop source".to_string()));
                unsafe {
                    CFRelease(tap as *const c_void);
                    drop(Box::from_raw(user_info as *mut CallbackContext));
                }
                return;
            }

            let _ = ready_tx.send(Ok(()));
            unsafe {
                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                CFRunLoopRun();

                CFRelease(source as *const c_void);
                CFRelease(tap as *const c_void);
                drop(Box::from_raw(user_info as *mut CallbackContext));
            }
        });

        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| anyhow!("timed out waiting for global hotkey listener startup"))?
            .map_err(|err| anyhow!(err))
    }

    pub fn has_global_input_access() -> bool {
        // Trust macOS preflight result directly so UI doesn't report "granted"
        // before TCC has actually allowed Input Monitoring.
        unsafe { CGPreflightListenEventAccess() }
    }

    pub fn request_global_input_access() -> bool {
        unsafe { CGRequestListenEventAccess() }
    }

    extern "C" fn event_tap_callback(
        _proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef {
        if event.is_null() || user_info.is_null() {
            return event;
        }

        let ctx = unsafe { &*(user_info as *const CallbackContext) };
        let target_keycode = ctx.config.keycode.load(Ordering::Acquire);
        let target_is_modifier = ctx.config.is_modifier.load(Ordering::Acquire);
        let target_flag_mask = ctx.config.flag_mask.load(Ordering::Acquire);

        let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
        if keycode != target_keycode {
            return event;
        }

        if target_is_modifier {
            if event_type != K_CG_EVENT_FLAGS_CHANGED {
                return event;
            }
            let flags = unsafe { CGEventGetFlags(event) };
            if (flags & target_flag_mask) != 0 {
                let _ = ctx.event_tx.send(HotkeyEvent::Pressed);
            } else {
                let _ = ctx.event_tx.send(HotkeyEvent::Released);
            }
        } else {
            match event_type {
                K_CG_EVENT_KEY_DOWN => {
                    let _ = ctx.event_tx.send(HotkeyEvent::Pressed);
                }
                K_CG_EVENT_KEY_UP => {
                    let _ = ctx.event_tx.send(HotkeyEvent::Released);
                }
                _ => {}
            }
        }

        event
    }

    fn event_mask_all() -> CGEventMask {
        (1u64 << K_CG_EVENT_KEY_DOWN)
            | (1u64 << K_CG_EVENT_KEY_UP)
            | (1u64 << K_CG_EVENT_FLAGS_CHANGED)
    }
}

// ── Public API (macOS) ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn spawn_hotkey_listener<F>(config: &'static HotkeyConfig, callback: F) -> Result<()>
where
    F: Fn(HotkeyEvent) + Send + Sync + 'static,
{
    macos_impl::spawn_hotkey_listener(config, callback)
}

#[cfg(target_os = "macos")]
pub fn has_global_input_access() -> bool {
    macos_impl::has_global_input_access()
}

#[cfg(target_os = "macos")]
pub fn request_global_input_access() -> bool {
    macos_impl::request_global_input_access()
}

// ── Public API (non-macOS stubs) ────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
pub fn spawn_hotkey_listener<F>(_config: &'static HotkeyConfig, _callback: F) -> Result<()>
where
    F: Fn(HotkeyEvent) + Send + Sync + 'static,
{
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn has_global_input_access() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn request_global_input_access() -> bool {
    true
}
