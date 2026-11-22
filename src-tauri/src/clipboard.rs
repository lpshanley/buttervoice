//! Puts text on the clipboard, only ever because the person asked.

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

pub fn copy(text: &str) -> bool {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
}
