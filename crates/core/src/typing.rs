#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingFailure {
    /// macOS has not been told this app may control the computer.
    NotPermitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// There was no text to type.
    NotAttempted,
    Typed,
    Failed(TypingFailure),
}

/// macOS reads at most this many UTF-16 units from one keyboard event.
const MAX_UNITS_PER_KEY_EVENT: usize = 20;

/// The text as the UTF-16 strings to put on successive keyboard events.
pub fn key_event_chunks(text: &str) -> Vec<Vec<u16>> {
    let mut chunks = Vec::new();
    let mut chunk = Vec::new();
    let mut units = [0_u16; 2];
    for character in text.chars() {
        let encoded = character.encode_utf16(&mut units);
        if chunk.len() + encoded.len() > MAX_UNITS_PER_KEY_EVENT {
            chunks.push(std::mem::take(&mut chunk));
        }
        chunk.extend_from_slice(encoded);
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}
