//! Types text into whichever app has the keyboard focus, by posting keyboard
//! events that carry the characters themselves. It needs the Accessibility
//! permission and leaves the clipboard alone.

use std::ffi::c_void;

use buttervoice_core::typing::{TypingFailure, key_event_chunks};

pub fn type_text(text: &str) -> Result<(), TypingFailure> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err(TypingFailure::NotPermitted);
    }
    for characters in key_event_chunks(text) {
        post_key_event(&characters, true);
        post_key_event(&characters, false);
    }
    Ok(())
}

fn post_key_event(characters: &[u16], key_down: bool) {
    unsafe {
        let event = CGEventCreateKeyboardEvent(std::ptr::null(), NO_PARTICULAR_KEY, key_down);
        if event.is_null() {
            return;
        }
        // Without this, a modifier the user happens to be holding would turn
        // the text into keyboard shortcuts in the other app.
        CGEventSetFlags(event, NO_MODIFIERS);
        CGEventKeyboardSetUnicodeString(event, characters.len(), characters.as_ptr());
        CGEventPost(HID_EVENT_TAP, event);
        CFRelease(event);
    }
}

const NO_PARTICULAR_KEY: u16 = 0;
const NO_MODIFIERS: u64 = 0;
const HID_EVENT_TAP: u32 = 0;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, characters: *const u16);
    fn CGEventPost(tap: u32, event: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
}
