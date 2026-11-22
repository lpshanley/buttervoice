//! A listen-only macOS event tap for modifier-key changes. It needs the
//! Input Monitoring permission and cannot delay or block other apps' keys.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use buttervoice_core::timeline::Timestamp;

pub enum TapEvent {
    /// The tap has just started. `flags` is the modifier state at that
    /// moment, in case a key is already held.
    Ready {
        flags: u64,
        at: Timestamp,
    },
    FlagsChanged {
        flags: u64,
        at: Timestamp,
    },
    /// macOS switched the tap off and it has been switched back on. Key
    /// events may have been missed, so `flags` is the modifier state now.
    Interrupted {
        flags: u64,
        at: Timestamp,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TapEnded {
    #[error("the key event tap could not be created; Input Monitoring may not be granted")]
    Unavailable,
    #[error("the key event tap's run loop stopped")]
    RunLoopStopped,
}

/// Runs the tap on the calling thread. It only returns once the tap can no
/// longer deliver events.
pub fn run(mut on_event: impl FnMut(TapEvent)) -> TapEnded {
    let context = TapContext {
        on_event: RefCell::new(&mut on_event),
        tap: Cell::new(std::ptr::null_mut()),
    };
    let tap = unsafe {
        CGEventTapCreate(
            SESSION_EVENT_TAP,
            HEAD_INSERT,
            LISTEN_ONLY,
            1 << FLAGS_CHANGED,
            on_tap_event,
            &raw const context as *mut c_void,
        )
    };
    if tap.is_null() {
        return TapEnded::Unavailable;
    }
    context.tap.set(tap);
    unsafe {
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        (context.on_event.borrow_mut())(TapEvent::Ready {
            flags: CGEventSourceFlagsState(HID_SYSTEM_STATE),
            at: now(),
        });
        CFRunLoopRun();
        CFMachPortInvalidate(tap);
        CFRelease(source);
        CFRelease(tap);
    }
    TapEnded::RunLoopStopped
}

pub fn now() -> Timestamp {
    Timestamp::from_nanos(unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) })
}

struct TapContext<'a> {
    on_event: RefCell<&'a mut (dyn FnMut(TapEvent) + 'a)>,
    tap: Cell<*mut c_void>,
}

unsafe extern "C" fn on_tap_event(
    _proxy: EventRef,
    event_type: u32,
    event: EventRef,
    user_info: *mut c_void,
) -> EventRef {
    // The context lives on the stack of `run`, which is blocked in the run
    // loop that calls us, on this same thread.
    let context = unsafe { &*(user_info as *const TapContext) };
    let tap_event = match event_type {
        FLAGS_CHANGED => TapEvent::FlagsChanged {
            flags: unsafe { CGEventGetFlags(event) },
            at: Timestamp::from_nanos(unsafe { CGEventGetTimestamp(event) }),
        },
        TAP_DISABLED_BY_TIMEOUT | TAP_DISABLED_BY_USER_INPUT => {
            unsafe { CGEventTapEnable(context.tap.get(), true) };
            TapEvent::Interrupted {
                flags: unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) },
                at: now(),
            }
        }
        _ => return event,
    };
    (context.on_event.borrow_mut())(tap_event);
    event
}

type EventRef = *mut c_void;
type TapCallback = unsafe extern "C" fn(EventRef, u32, EventRef, *mut c_void) -> EventRef;

const SESSION_EVENT_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const LISTEN_ONLY: u32 = 1;
const FLAGS_CHANGED: u32 = 12;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const HID_SYSTEM_STATE: i32 = 1;
/// The clock CGEventGetTimestamp reports in: nanoseconds of uptime.
const CLOCK_UPTIME_RAW: u32 = 8;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetFlags(event: EventRef) -> u64;
    fn CGEventGetTimestamp(event: EventRef) -> u64;
    fn CGEventSourceFlagsState(state: i32) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFMachPortInvalidate(port: *mut c_void);
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRun();
    fn CFRelease(object: *const c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

unsafe extern "C" {
    fn clock_gettime_nsec_np(clock_id: u32) -> u64;
}
