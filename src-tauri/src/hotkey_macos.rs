use std::sync::atomic::AtomicU64;
use std::sync::RwLock;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[cfg(all(target_os = "macos", not(test)))]
fn physical_key_is_down(keycode: i64) -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventSourceKeyState(source_state: i32, keycode: u16) -> bool;
        fn CGEventSourceFlagsState(source_state: i32) -> u64;
    }
    query_physical_key_state(
        keycode,
        |keycode| unsafe { CGEventSourceKeyState(1, keycode as u16) },
        || unsafe { CGEventSourceFlagsState(1) },
    )
}

// Unit tests supply held/up observations directly. Reading HID state here
// would make otherwise synthetic tests depend on the user's keyboard and
// permissions, and can block in a headless/sandboxed test process.
#[cfg(any(not(target_os = "macos"), test))]
fn physical_key_is_down(_keycode: i64) -> bool {
    false
}

// A process-wide counter keeps press ids unique even if the supervisor has to
// create a second listener after the first one fails during startup.
static GLOBAL_PRESS_ID: AtomicU64 = AtomicU64::new(0);

// ── Data model ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyReleaseReason {
    KeyReleased,
    MissedRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyInterruptionReason {
    ListenerDisabled,
    ListenerLost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub key: HotkeyKey,
    pub mode: DictationMode,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed {
        binding: HotkeyBinding,
        press_id: u64,
    },
    Released {
        binding: HotkeyBinding,
        press_id: u64,
        reason: HotkeyReleaseReason,
    },
    Interrupted {
        revision: u64,
        through_press_id: u64,
        reason: HotkeyInterruptionReason,
    },
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
                // CGEventFlags has side-specific bits. The aggregate Option
                // mask would misread a right-side release while left Option is
                // still held.
                flag_mask: 0x40,
                display_label: "Right Option (⌥)",
            },
            Self::LeftOption => HotkeySpec {
                keycode: 58,
                is_modifier: true,
                flag_mask: 0x20,
                display_label: "Left Option (⌥)",
            },
            Self::RightCommand => HotkeySpec {
                keycode: 54,
                is_modifier: true,
                flag_mask: 0x10,
                display_label: "Right Command (⌘)",
            },
            Self::RightControl => HotkeySpec {
                keycode: 62,
                is_modifier: true,
                flag_mask: 0x2000,
                display_label: "Right Control (⌃)",
            },
            Self::LeftControl => HotkeySpec {
                keycode: 59,
                is_modifier: true,
                flag_mask: 0x1,
                display_label: "Left Control (⌃)",
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
                flag_mask: modifier_flag_mask(keycode),
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

// A snapshot is deliberately stored as one value. The callback can therefore
// never observe a key from one update and a mode/revision from another update.
pub struct HotkeyConfig {
    binding: RwLock<ConfigSnapshot>,
}

#[derive(Debug, Clone, Copy)]
struct ConfigSnapshot {
    binding: HotkeyBinding,
    initially_down: bool,
}

impl HotkeyConfig {
    pub fn new(key: &HotkeyKey, mode: DictationMode) -> Self {
        Self {
            binding: RwLock::new(ConfigSnapshot {
                binding: HotkeyBinding {
                    key: *key,
                    mode,
                    revision: 0,
                },
                // The native owner samples held keys when the tap is first
                // installed, after input permission is available.
                initially_down: false,
            }),
        }
    }

    pub fn update(&self, key: &HotkeyKey, mode: DictationMode) {
        // Never hold the snapshot lock across a WindowServer/HID query: the
        // event tap callback must be able to read configuration promptly.
        let initially_down = physical_key_is_down(key.spec().keycode);
        let mut binding = self
            .binding
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        binding.binding.key = *key;
        binding.binding.mode = mode;
        binding.binding.revision = binding.binding.revision.wrapping_add(1);
        binding.initially_down = initially_down;
    }

    pub fn snapshot(&self) -> HotkeyBinding {
        self.binding
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .binding
    }

    fn snapshot_with_initial_state(&self) -> (HotkeyBinding, bool) {
        let snapshot = self
            .binding
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (snapshot.binding, snapshot.initially_down)
    }
}

// ── Validation ──────────────────────────────────────────────────────────────

pub fn validate_hotkey(key: &HotkeyKey) -> Result<()> {
    if let HotkeyKey::Custom { keycode, .. } = key {
        if *keycode < 0 || *keycode > 127 {
            return Err(anyhow!("invalid keycode {keycode} (must be 0–127)"));
        }
        if key.spec().is_modifier && modifier_flag_mask(*keycode) == 0 {
            return Err(anyhow!("keycode {keycode} is not a supported modifier key"));
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

// ── Platform-independent event state ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputEdge {
    Down,
    Up,
}

fn modifier_edge(flags: u64, flag_mask: u64) -> InputEdge {
    if flag_mask != 0 && (flags & flag_mask) != 0 {
        InputEdge::Down
    } else {
        InputEdge::Up
    }
}

// Device-specific flag bits from the macOS SDK's IOLLEvent.h. Caps Lock's
// stateless bit represents the physical key rather than its toggled setting.
fn modifier_flag_mask(keycode: i64) -> u64 {
    match keycode {
        54 => 0x10,
        55 => 0x08,
        56 => 0x02,
        57 => 0x80,
        58 => 0x20,
        59 => 0x01,
        60 => 0x04,
        61 => 0x40,
        62 => 0x2000,
        63 => 0x0080_0000,
        _ => 0,
    }
}

fn query_physical_key_state(
    keycode: i64,
    query_key: impl FnOnce(i64) -> bool,
    query_flags: impl FnOnce() -> u64,
) -> bool {
    if !(0..=127).contains(&keycode) {
        return false;
    }
    // Modifier keys produce flags-changed events. The ordinary key-state
    // table can report them as up throughout a hold, causing the watchdog to
    // synthesize a release after two polls. Use the same side-specific flags
    // as the event callback for polling, startup, and reconfiguration.
    let flag_mask = modifier_flag_mask(keycode);
    if flag_mask != 0 {
        return modifier_edge(query_flags(), flag_mask) == InputEdge::Down;
    }
    query_key(keycode)
}

fn decode_key_event(
    binding: HotkeyBinding,
    event_type: u32,
    keycode: i64,
    flags: u64,
    autorepeat: bool,
) -> Option<InputEdge> {
    let spec = binding.key.spec();
    if keycode != spec.keycode {
        return None;
    }
    if spec.is_modifier {
        return (event_type == 12 && spec.flag_mask != 0)
            .then(|| modifier_edge(flags, spec.flag_mask));
    }
    match event_type {
        10 if !autorepeat => Some(InputEdge::Down),
        11 => Some(InputEdge::Up),
        _ => None,
    }
}

/// Edge detection is kept separate from CoreGraphics so it can be exercised
/// without a macOS event tap. In particular, modifier state is keyed by the
/// changed key's own keycode; aggregate modifier flags are not used to decide
/// whether a side-specific modifier was released.
#[derive(Debug)]
struct HotkeyState {
    binding: HotkeyBinding,
    pressed: bool,
    press_id: u64,
    missed_up_checks: u8,
    awaiting_up: bool,
}

impl HotkeyState {
    fn new(binding: HotkeyBinding) -> Self {
        Self {
            binding,
            pressed: false,
            press_id: 0,
            missed_up_checks: 0,
            awaiting_up: false,
        }
    }

    fn sync_binding(&mut self, binding: HotkeyBinding) {
        self.sync_binding_with_physical_state(binding, false);
    }

    fn sync_binding_with_physical_state(&mut self, binding: HotkeyBinding, key_is_down: bool) {
        if self.binding != binding {
            // A key held during reconfiguration must not become a press of
            // the new key. The next fresh down edge after an observed up is
            // required. The native watchdog supplies the physical state here.
            self.binding = binding;
            self.pressed = false;
            self.press_id = 0;
            self.missed_up_checks = 0;
            self.awaiting_up = key_is_down;
        }
    }

    fn rearm(&mut self, binding: HotkeyBinding, key_is_down: bool) {
        self.binding = binding;
        self.pressed = false;
        self.press_id = 0;
        self.missed_up_checks = 0;
        self.awaiting_up = key_is_down;
    }

    fn edge(&mut self, binding: HotkeyBinding, edge: InputEdge) -> Option<HotkeyEvent> {
        self.sync_binding(binding);
        if self.awaiting_up {
            if edge == InputEdge::Up {
                self.awaiting_up = false;
            }
            return None;
        }
        match edge {
            InputEdge::Down if !self.pressed => {
                self.pressed = true;
                self.missed_up_checks = 0;
                self.press_id = GLOBAL_PRESS_ID
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    .wrapping_add(1);
                Some(HotkeyEvent::Pressed {
                    binding: self.binding,
                    press_id: self.press_id,
                })
            }
            InputEdge::Down => None, // autorepeat / duplicate flags-changed down
            InputEdge::Up if self.pressed => {
                self.pressed = false;
                self.missed_up_checks = 0;
                Some(HotkeyEvent::Released {
                    binding: self.binding,
                    press_id: self.press_id,
                    reason: HotkeyReleaseReason::KeyReleased,
                })
            }
            InputEdge::Up => None,
        }
    }

    fn watchdog_check(&mut self, binding: HotkeyBinding, key_is_down: bool) -> Option<HotkeyEvent> {
        self.sync_binding_with_physical_state(binding, key_is_down);
        if self.awaiting_up {
            if !key_is_down {
                self.awaiting_up = false;
            }
            return None;
        }
        if !self.pressed || key_is_down {
            self.missed_up_checks = 0;
            return None;
        }

        self.missed_up_checks = self.missed_up_checks.saturating_add(1);
        if self.missed_up_checks < 2 {
            return None;
        }

        self.pressed = false;
        self.missed_up_checks = 0;
        Some(HotkeyEvent::Released {
            binding: self.binding,
            press_id: self.press_id,
            reason: HotkeyReleaseReason::MissedRelease,
        })
    }

    fn interrupt(&mut self, reason: HotkeyInterruptionReason) -> HotkeyEvent {
        // Keep the last press id after a normal release. Toggle mode may still
        // have an active recording when the listener is interrupted, so the
        // owner needs this id to correlate the interruption with that session.
        let through_press_id = self.press_id;
        self.pressed = false;
        self.press_id = 0;
        self.missed_up_checks = 0;
        self.awaiting_up = false;
        HotkeyEvent::Interrupted {
            revision: self.binding.revision,
            through_press_id,
            reason,
        }
    }
}

// ── macOS implementation ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::{
        decode_key_event, physical_key_is_down, HotkeyConfig, HotkeyEvent,
        HotkeyInterruptionReason, HotkeyState,
    };
    use anyhow::{anyhow, Context, Result};
    use crossbeam_channel::{unbounded, Sender};
    use std::ffi::c_void;
    use std::ptr;
    use std::time::{Duration, Instant};

    type NativeRef = *mut c_void;
    type CFStringRef = *const c_void;
    type TapCallback = extern "C" fn(NativeRef, u32, NativeRef, NativeRef) -> NativeRef;
    const WATCHDOG_INTERVAL: Duration = Duration::from_millis(100);
    const RETRY_INTERVAL: Duration = Duration::from_millis(500);
    const TAP_DISABLED_TIMEOUT: u32 = 0xffff_fffe;
    const TAP_DISABLED_USER_INPUT: u32 = 0xffff_ffff;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            mask: u64,
            callback: TapCallback,
            info: NativeRef,
        ) -> NativeRef;
        fn CGEventTapEnable(tap: NativeRef, enable: bool);
        fn CGEventTapIsEnabled(tap: NativeRef) -> bool;
        fn CGEventGetIntegerValueField(event: NativeRef, field: i32) -> i64;
        fn CGEventGetFlags(event: NativeRef) -> u64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: NativeRef,
            order: isize,
        ) -> NativeRef;
        fn CFMachPortIsValid(port: NativeRef) -> bool;
        fn CFMachPortInvalidate(port: NativeRef);
        fn CFRunLoopGetCurrent() -> NativeRef;
        fn CFRunLoopAddSource(run_loop: NativeRef, source: NativeRef, mode: CFStringRef);
        fn CFRunLoopRemoveSource(run_loop: NativeRef, source: NativeRef, mode: CFStringRef);
        fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source: bool) -> i32;
        fn CFRelease(value: *const c_void);
        static kCFRunLoopCommonModes: CFStringRef;
        static kCFRunLoopDefaultMode: CFStringRef;
    }

    // Native callbacks, configuration synchronization, key polling, and tap
    // destruction all run on one owner thread. No borrowed native pointer or
    // out-of-date key-state snapshot can race a second watchdog thread.
    struct CallbackContext {
        event_tx: Sender<HotkeyEvent>,
        config: &'static HotkeyConfig,
        state: HotkeyState,
        restart: bool,
        loss_announced: bool,
        consumer_alive: bool,
    }

    impl CallbackContext {
        fn sync_config(&mut self) {
            let (binding, initially_down) = self.config.snapshot_with_initial_state();
            self.state
                .sync_binding_with_physical_state(binding, initially_down);
        }

        fn send(&mut self, event: HotkeyEvent) {
            // Unbounded channel delivery only queues work; audio never runs
            // inside the event tap callback.
            if self.event_tx.try_send(event).is_err() {
                self.consumer_alive = false;
            }
        }

        fn announce_loss(&mut self, reason: HotkeyInterruptionReason) {
            self.restart = true;
            if !self.loss_announced {
                self.loss_announced = true;
                let event = self.state.interrupt(reason);
                self.send(event);
            }
        }

        fn watchdog(&mut self) {
            self.sync_config();
            let binding = self.state.binding;
            if let Some(event) = self
                .state
                .watchdog_check(binding, physical_key_is_down(binding.key.spec().keycode))
            {
                self.send(event);
            }
        }
    }

    struct EventTap {
        tap: NativeRef,
        source: NativeRef,
        run_loop: NativeRef,
    }

    impl EventTap {
        // The caller keeps the boxed context alive until this tap is dropped.
        fn create(context: &mut CallbackContext) -> Result<Self> {
            unsafe {
                let tap = CGEventTapCreate(
                    0,
                    0,
                    1,
                    (1 << 10) | (1 << 11) | (1 << 12),
                    event_tap_callback,
                    context as *mut _ as NativeRef,
                );
                if tap.is_null() {
                    return Err(anyhow!("failed creating global event tap"));
                }
                let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
                if source.is_null() {
                    CFMachPortInvalidate(tap);
                    CFRelease(tap);
                    return Err(anyhow!("failed creating event tap run loop source"));
                }
                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                Ok(Self {
                    tap,
                    source,
                    run_loop,
                })
            }
        }

        fn is_healthy(&self) -> bool {
            unsafe {
                CFMachPortIsValid(self.tap)
                    && CGEventTapIsEnabled(self.tap)
                    && CGPreflightListenEventAccess()
            }
        }
    }

    impl Drop for EventTap {
        fn drop(&mut self) {
            unsafe {
                CGEventTapEnable(self.tap, false);
                CFRunLoopRemoveSource(self.run_loop, self.source, kCFRunLoopCommonModes);
                CFMachPortInvalidate(self.tap);
                CFRelease(self.source);
                CFRelease(self.tap);
            }
        }
    }

    pub fn spawn_hotkey_listener<F>(config: &'static HotkeyConfig, callback: F) -> Result<()>
    where
        F: Fn(HotkeyEvent) + Send + Sync + 'static,
    {
        let (event_tx, event_rx) = unbounded();
        std::thread::Builder::new()
            .name("dictation-shortcuts".into())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    callback(event);
                }
            })
            .context("failed spawning shortcut event consumer")?;
        // This acknowledges supervisor creation, not permission or tap readiness.
        // The single supervisor owns all startup retries. There is no startup
        // timeout that could leave a late successful listener running beside a
        // replacement listener.
        std::thread::Builder::new()
            .name("dictation-input".into())
            .spawn(move || {
                supervise_listener(config, event_tx);
            })
            .context("failed spawning shortcut listener supervisor")?;
        Ok(())
    }

    fn supervise_listener(config: &'static HotkeyConfig, event_tx: Sender<HotkeyEvent>) {
        let mut context = Box::new(CallbackContext {
            event_tx,
            config,
            state: HotkeyState::new(config.snapshot()),
            restart: false,
            loss_announced: false,
            consumer_alive: true,
        });
        while context.consumer_alive {
            context.sync_config();
            if !has_global_input_access() {
                context.announce_loss(HotkeyInterruptionReason::ListenerLost);
                std::thread::sleep(RETRY_INTERVAL);
                continue;
            }
            let tap = match EventTap::create(&mut context) {
                Ok(tap) => tap,
                Err(err) => {
                    if !context.loss_announced {
                        eprintln!("hotkey listener unavailable: {err:#}");
                    }
                    context.announce_loss(HotkeyInterruptionReason::ListenerLost);
                    std::thread::sleep(RETRY_INTERVAL);
                    continue;
                }
            };
            let binding = config.snapshot();
            context
                .state
                .rearm(binding, physical_key_is_down(binding.key.spec().keycode));
            context.restart = false;
            context.loss_announced = false;
            tracing::info!(revision = binding.revision, "hotkey_listener_ready");
            let mut next_check = Instant::now() + WATCHDOG_INTERVAL;
            while context.consumer_alive && !context.restart {
                let now = Instant::now();
                if now >= next_check {
                    if !tap.is_healthy() {
                        context.announce_loss(HotkeyInterruptionReason::ListenerLost);
                        break;
                    }
                    context.watchdog();
                    next_check = Instant::now() + WATCHDOG_INTERVAL;
                }
                let wait = next_check
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64();
                let result = unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, wait, true) };
                // Finished/stopped means the source or run loop was lost.
                // Timed out/handled source are the normal polling cases.
                if result == 1 || result == 2 {
                    context.announce_loss(HotkeyInterruptionReason::ListenerLost);
                }
            }
            // Remove/invalidate the source before recreating the tap or
            // dropping its callback context.
            drop(tap);
            if context.consumer_alive {
                std::thread::sleep(RETRY_INTERVAL);
            }
        }
    }

    pub fn has_global_input_access() -> bool {
        unsafe { CGPreflightListenEventAccess() }
    }
    pub fn request_global_input_access() -> bool {
        unsafe { CGRequestListenEventAccess() }
    }

    extern "C" fn event_tap_callback(
        _proxy: NativeRef,
        event_type: u32,
        event: NativeRef,
        user_info: NativeRef,
    ) -> NativeRef {
        if user_info.is_null() {
            return event;
        }
        // Only the native owner thread accesses this boxed context, and it
        // outlives all registered taps and their run loop sources.
        let context = unsafe { &mut *(user_info as *mut CallbackContext) };
        if event_type == TAP_DISABLED_TIMEOUT || event_type == TAP_DISABLED_USER_INPUT {
            context.announce_loss(HotkeyInterruptionReason::ListenerDisabled);
            return event;
        }
        if event.is_null() || context.restart {
            return event;
        }
        context.sync_config();
        let binding = context.state.binding;
        let keycode = unsafe { CGEventGetIntegerValueField(event, 9) };
        let flags = unsafe { CGEventGetFlags(event) };
        let autorepeat = unsafe { CGEventGetIntegerValueField(event, 8) != 0 };
        if let Some(edge) = decode_key_event(binding, event_type, keycode, flags, autorepeat) {
            if let Some(action) = context.state.edge(binding, edge) {
                context.send(action);
            }
        }
        event
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::hotkey_macos::{DictationMode, HotkeyKey};

        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn CGEventCreateKeyboardEvent(
                source: *const c_void,
                keycode: u16,
                down: bool,
            ) -> NativeRef;
            fn CGEventSetFlags(event: NativeRef, flags: u64);
            fn CGEventSetIntegerValueField(event: NativeRef, field: i32, value: i64);
        }

        fn context(
            key: HotkeyKey,
            mode: DictationMode,
        ) -> (CallbackContext, crossbeam_channel::Receiver<HotkeyEvent>) {
            let config = Box::leak(Box::new(HotkeyConfig::new(&key, mode)));
            let (event_tx, event_rx) = unbounded();
            (
                CallbackContext {
                    event_tx,
                    config,
                    state: HotkeyState::new(config.snapshot()),
                    restart: false,
                    loss_announced: false,
                    consumer_alive: true,
                },
                event_rx,
            )
        }

        // Construct real CoreGraphics event objects, but never post them to
        // the OS or open a microphone/event tap.
        fn deliver(
            context: &mut CallbackContext,
            keycode: u16,
            kind: u32,
            flags: u64,
            repeat: bool,
        ) {
            unsafe {
                let event = CGEventCreateKeyboardEvent(ptr::null(), keycode, kind != 11);
                assert!(!event.is_null());
                CGEventSetFlags(event, flags);
                CGEventSetIntegerValueField(event, 8, i64::from(repeat));
                event_tap_callback(ptr::null_mut(), kind, event, context as *mut _ as NativeRef);
                CFRelease(event);
            }
        }

        fn assert_one_press_and_release(events: Vec<HotkeyEvent>) {
            assert_eq!(events.len(), 2, "unexpected input actions: {events:?}");
            match (events[0], events[1]) {
                (
                    HotkeyEvent::Pressed { binding, press_id },
                    HotkeyEvent::Released {
                        binding: released,
                        press_id: released_id,
                        ..
                    },
                ) => {
                    assert_eq!(binding, released);
                    assert_eq!(press_id, released_id);
                }
                _ => panic!("expected one press/release pair: {events:?}"),
            }
        }

        #[test]
        fn native_modifier_overlap_releases_only_the_configured_side() {
            for (key, other_keycode, own_mask, other_mask, aggregate) in [
                (HotkeyKey::RightOption, 58, 0x40, 0x20, 0x80000),
                (HotkeyKey::LeftOption, 61, 0x20, 0x40, 0x80000),
                (HotkeyKey::RightControl, 59, 0x2000, 0x01, 0x40000),
                (HotkeyKey::LeftControl, 62, 0x01, 0x2000, 0x40000),
                (HotkeyKey::RightCommand, 55, 0x10, 0x08, 0x100000),
            ] {
                let (mut context, events) = context(key, DictationMode::PushToTalk);
                let own = key.spec().keycode as u16;
                deliver(&mut context, own, 12, aggregate | own_mask, false);
                deliver(
                    &mut context,
                    other_keycode,
                    12,
                    aggregate | own_mask | other_mask,
                    false,
                );
                deliver(&mut context, own, 12, aggregate | other_mask, false);
                deliver(&mut context, other_keycode, 12, 0, false);
                assert_one_press_and_release(events.try_iter().collect());
            }
        }

        #[test]
        fn native_queued_rapid_taps_use_event_time_flags() {
            let (mut context, events) = context(HotkeyKey::RightOption, DictationMode::PushToTalk);
            for _ in 0..2 {
                deliver(&mut context, 61, 12, 0x80040, false);
                deliver(&mut context, 61, 12, 0, false);
            }
            let events: Vec<_> = events.try_iter().collect();
            assert_eq!(events.len(), 4);
            assert_one_press_and_release(events[..2].to_vec());
            assert_one_press_and_release(events[2..].to_vec());
            assert_ne!(events[0], events[2]);
        }

        #[test]
        fn native_custom_key_autorepeat_and_unrelated_events_are_ignored() {
            let (mut context, events) = context(
                HotkeyKey::Custom {
                    keycode: 96,
                    is_modifier: false,
                },
                DictationMode::Toggle,
            );
            deliver(&mut context, 96, 10, 0, false);
            deliver(&mut context, 96, 10, 0, true);
            deliver(&mut context, 96, 10, 0, true);
            deliver(&mut context, 95, 11, 0, false);
            deliver(&mut context, 96, 11, 0, false);
            assert_one_press_and_release(events.try_iter().collect());
        }

        #[test]
        fn native_fn_and_custom_modifier_events_preserve_releases() {
            for key in [
                HotkeyKey::Fn,
                HotkeyKey::Custom {
                    keycode: 56,
                    is_modifier: true,
                },
            ] {
                let spec = key.spec();
                let (mut context, events) = context(key, DictationMode::PushToTalk);
                deliver(&mut context, spec.keycode as u16, 12, spec.flag_mask, false);
                deliver(&mut context, spec.keycode as u16, 12, 0, false);
                assert_one_press_and_release(events.try_iter().collect());
            }
        }

        #[test]
        fn native_disabled_notification_without_event_interrupts_released_toggle() {
            for kind in [TAP_DISABLED_TIMEOUT, TAP_DISABLED_USER_INPUT] {
                let (mut context, events) = context(HotkeyKey::RightOption, DictationMode::Toggle);
                deliver(&mut context, 61, 12, 0x80040, false);
                deliver(&mut context, 61, 12, 0, false);
                event_tap_callback(
                    ptr::null_mut(),
                    kind,
                    ptr::null_mut(),
                    &mut context as *mut _ as NativeRef,
                );
                event_tap_callback(
                    ptr::null_mut(),
                    kind,
                    ptr::null_mut(),
                    &mut context as *mut _ as NativeRef,
                );
                assert!(context.restart);
                let events: Vec<_> = events.try_iter().collect();
                assert_eq!(events.len(), 3);
                match (events[0], events[2]) {
                    (
                        HotkeyEvent::Pressed { binding, press_id },
                        HotkeyEvent::Interrupted {
                            revision,
                            through_press_id,
                            ..
                        },
                    ) => {
                        assert_eq!(binding.revision, revision);
                        assert_eq!(press_id, through_press_id);
                    }
                    _ => panic!("interruption lost session identity: {events:?}"),
                }
            }
        }

        #[test]
        fn recreated_listener_never_reuses_a_press_identity() {
            let (mut first, first_events) =
                context(HotkeyKey::RightOption, DictationMode::PushToTalk);
            deliver(&mut first, 61, 12, 0x80040, false);
            let (mut second, second_events) =
                context(HotkeyKey::RightOption, DictationMode::PushToTalk);
            deliver(&mut second, 61, 12, 0x80040, false);
            match (first_events.recv().unwrap(), second_events.recv().unwrap()) {
                (
                    HotkeyEvent::Pressed {
                        press_id: first, ..
                    },
                    HotkeyEvent::Pressed {
                        press_id: second, ..
                    },
                ) => assert!(second > first),
                _ => panic!("expected two presses"),
            }
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(key: HotkeyKey) -> HotkeyState {
        HotkeyState::new(HotkeyBinding {
            key,
            mode: DictationMode::PushToTalk,
            revision: 0,
        })
    }

    #[test]
    fn normal_press_release_has_one_id() {
        let mut state = state(HotkeyKey::RightOption);
        let binding = state.binding;
        assert!(matches!(
            state.edge(binding, InputEdge::Down),
            Some(HotkeyEvent::Pressed { press_id, .. }) if press_id > 0
        ));
        assert!(matches!(
            state.edge(binding, InputEdge::Up),
            Some(HotkeyEvent::Released {
                press_id,
                reason: HotkeyReleaseReason::KeyReleased,
                ..
            }) if press_id > 0
        ));
    }

    #[test]
    fn duplicate_down_and_autorepeat_do_not_press_twice() {
        let mut state = state(HotkeyKey::Custom {
            keycode: 0,
            is_modifier: false,
        });
        let binding = state.binding;
        assert!(state.edge(binding, InputEdge::Down).is_some());
        assert!(state.edge(binding, InputEdge::Down).is_none());
        assert!(state.edge(binding, InputEdge::Down).is_none());
        assert!(state.edge(binding, InputEdge::Up).is_some());
    }

    #[test]
    fn left_right_modifier_overlap_is_independent() {
        let mut right = state(HotkeyKey::RightOption);
        let binding = right.binding;
        assert!(right.edge(binding, InputEdge::Down).is_some());
        // A left-side flags change does not release the right key. The right
        // release then emits exactly one release even if left stays held.
        assert!(right.edge(binding, InputEdge::Down).is_none());
        assert!(right.edge(binding, InputEdge::Up).is_some());
        assert!(right.edge(binding, InputEdge::Up).is_none());
    }

    #[test]
    fn side_specific_flags_decode_release_while_other_side_is_held() {
        assert_eq!(modifier_edge(0x40, 0x40), InputEdge::Down); // right Option
        assert_eq!(modifier_edge(0x20, 0x40), InputEdge::Up); // left Option only
        assert_eq!(modifier_edge(0x60, 0x40), InputEdge::Down); // both held
    }

    #[test]
    fn watchdog_requires_two_up_observations() {
        let mut state = state(HotkeyKey::RightOption);
        let binding = state.binding;
        assert!(state.edge(binding, InputEdge::Down).is_some());
        assert!(state.watchdog_check(binding, false).is_none());
        assert!(matches!(
            state.watchdog_check(binding, false),
            Some(HotkeyEvent::Released {
                reason: HotkeyReleaseReason::MissedRelease,
                ..
            })
        ));
    }

    #[test]
    fn watchdog_keeps_held_modifiers_recording_when_key_table_reports_up() {
        for keycode in [54, 55, 56, 57, 58, 59, 60, 61, 62, 63] {
            let mut state = state(HotkeyKey::Custom {
                keycode,
                is_modifier: true,
            });
            let binding = state.binding;
            let held_flags = modifier_flag_mask(keycode);
            let press_id = match state.edge(binding, InputEdge::Down) {
                Some(HotkeyEvent::Pressed { press_id, .. }) => press_id,
                _ => panic!("expected press"),
            };

            // Modifier flags remain down throughout a long hold even when
            // the ordinary key-state table reports false on every poll.
            for _ in 0..20 {
                let held = query_physical_key_state(keycode, |_| false, || held_flags);
                assert!(
                    state.watchdog_check(binding, held).is_none(),
                    "watchdog released held modifier {keycode}"
                );
            }
            assert!(matches!(
                state.edge(binding, InputEdge::Up),
                Some(HotkeyEvent::Released {
                    press_id: released_id,
                    reason: HotkeyReleaseReason::KeyReleased,
                    ..
                }) if released_id == press_id
            ));
        }
    }

    #[test]
    fn modifier_polling_recovers_missed_release_with_opposite_side_held() {
        for (keycode, other_mask, aggregate) in [
            (54, 0x08, 0x10_0000),
            (55, 0x10, 0x10_0000),
            (56, 0x04, 0x02_0000),
            (60, 0x02, 0x02_0000),
            (58, 0x40, 0x08_0000),
            (61, 0x20, 0x08_0000),
            (59, 0x2000, 0x04_0000),
            (62, 0x01, 0x04_0000),
            // Caps Lock may remain toggled after the physical key is up.
            (57, 0, 0x01_0000),
            (63, 0, 0),
        ] {
            let mut state = state(HotkeyKey::Custom {
                keycode,
                is_modifier: true,
            });
            let binding = state.binding;
            assert!(state.edge(binding, InputEdge::Down).is_some());
            let both_held = modifier_flag_mask(keycode) | other_mask | aggregate;
            let held = query_physical_key_state(keycode, |_| false, || both_held);
            assert!(held, "modifier {keycode} must be held");
            assert!(state.watchdog_check(binding, held).is_none());

            let released = query_physical_key_state(keycode, |_| true, || other_mask | aggregate);
            assert!(!released, "modifier {keycode} must be released");
            assert!(state.watchdog_check(binding, released).is_none());
            assert!(matches!(
                state.watchdog_check(binding, released),
                Some(HotkeyEvent::Released {
                    reason: HotkeyReleaseReason::MissedRelease,
                    ..
                })
            ));
            assert!(state.edge(binding, InputEdge::Up).is_none());
        }
    }

    #[test]
    fn ordinary_key_polling_uses_key_table_and_invalid_keys_are_not_queried() {
        for held in [false, true] {
            assert_eq!(
                query_physical_key_state(
                    96,
                    |keycode| {
                        assert_eq!(keycode, 96);
                        held
                    },
                    || panic!("ordinary keys must use the key-state table"),
                ),
                held
            );
        }
        for invalid in [-1, 128] {
            assert!(!query_physical_key_state(
                invalid,
                |_| panic!("invalid keycode must not be queried"),
                || panic!("invalid keycode must not be queried"),
            ));
        }
    }

    #[test]
    fn interruption_reports_active_press_and_clears_state() {
        let mut state = state(HotkeyKey::RightOption);
        let binding = state.binding;
        assert!(state.edge(binding, InputEdge::Down).is_some());
        assert!(matches!(
            state.interrupt(HotkeyInterruptionReason::ListenerDisabled),
            HotkeyEvent::Interrupted {
                revision: 0,
                through_press_id,
                reason: HotkeyInterruptionReason::ListenerDisabled,
            } if through_press_id > 0
        ));
        assert!(state.edge(binding, InputEdge::Up).is_none());
    }

    #[test]
    fn interruption_after_release_keeps_toggle_press_id() {
        let mut state = state(HotkeyKey::RightOption);
        let binding = state.binding;
        let press_id = match state.edge(binding, InputEdge::Down) {
            Some(HotkeyEvent::Pressed { press_id, .. }) => press_id,
            _ => panic!("expected press"),
        };
        assert!(matches!(
            state.edge(binding, InputEdge::Up),
            Some(HotkeyEvent::Released { .. })
        ));
        assert!(matches!(
            state.interrupt(HotkeyInterruptionReason::ListenerLost),
            HotkeyEvent::Interrupted {
                through_press_id: interrupted_id,
                ..
            } if interrupted_id == press_id
        ));
    }

    #[test]
    fn reconfiguration_requires_fresh_press_and_revision_changes() {
        let mut state = state(HotkeyKey::RightOption);
        let old = state.binding;
        assert!(state.edge(old, InputEdge::Down).is_some());
        let new = HotkeyBinding {
            key: HotkeyKey::Fn,
            mode: DictationMode::Toggle,
            revision: 1,
        };
        state.sync_binding(new);
        assert!(state.edge(new, InputEdge::Up).is_none());
        assert!(matches!(
            state.edge(new, InputEdge::Down),
            Some(HotkeyEvent::Pressed {
                binding: HotkeyBinding { revision: 1, .. },
                press_id,
            })
            if press_id > 0
        ));
    }

    #[test]
    fn held_key_after_reconfiguration_is_blocked_until_up() {
        let mut state = state(HotkeyKey::RightOption);
        let new = HotkeyBinding {
            key: HotkeyKey::Fn,
            mode: DictationMode::PushToTalk,
            revision: 1,
        };
        state.sync_binding_with_physical_state(new, true);
        assert!(state.edge(new, InputEdge::Down).is_none());
        assert!(state.edge(new, InputEdge::Down).is_none());
        assert!(state.edge(new, InputEdge::Up).is_none());
        assert!(matches!(
            state.edge(new, InputEdge::Down),
            Some(HotkeyEvent::Pressed { .. })
        ));
    }

    #[test]
    fn a_new_key_that_was_up_accepts_first_down() {
        let mut state = state(HotkeyKey::RightOption);
        let new = HotkeyBinding {
            key: HotkeyKey::Fn,
            mode: DictationMode::PushToTalk,
            revision: 1,
        };
        state.sync_binding_with_physical_state(new, false);
        assert!(matches!(
            state.edge(new, InputEdge::Down),
            Some(HotkeyEvent::Pressed { .. })
        ));
    }
}
