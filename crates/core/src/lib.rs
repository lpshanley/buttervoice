//! Must not depend on Tauri, audio devices or OS APIs: everything here has to
//! run in tests and in fixture replays.

pub mod api_key;
pub mod capture;
pub mod hotkey;
pub mod key_dictation;
pub mod language;
pub mod recording;
pub mod timeline;
pub mod transcription;
pub mod typing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DictationId(pub u64);
