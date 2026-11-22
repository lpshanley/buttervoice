//! Must not depend on Tauri, audio devices or OS APIs: everything here has to
//! run in tests and in fixture replays.

pub mod hotkey;
pub mod recording;
pub mod timeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DictationId(pub u64);
