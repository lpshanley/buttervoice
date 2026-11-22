//! The recording session a person is walking through, if any, and where
//! finished sets are kept.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use buttervoice_core::language::Language;
use buttervoice_samples::session::{Session, Take};
use buttervoice_samples::{RecordedWith, script::script};

#[derive(serde::Serialize)]
pub struct PromptState {
    kind: &'static str,
    say: Option<String>,
    instruction: Option<String>,
    conditions: Option<String>,
}

#[derive(serde::Serialize)]
pub struct TakeState {
    heard: Option<String>,
    seconds: f64,
}

#[derive(serde::Serialize)]
pub struct State {
    active: bool,
    position: usize,
    total: usize,
    prompt: Option<PromptState>,
    take: Option<TakeState>,
}

pub struct Recorder {
    sets_folder: PathBuf,
    session: Mutex<Option<Session>>,
}

impl Recorder {
    pub fn new(sets_folder: PathBuf) -> Self {
        Self {
            sets_folder,
            session: Mutex::new(None),
        }
    }

    pub fn active(&self) -> bool {
        self.session().is_some()
    }

    pub fn start(&self, recorded_with: RecordedWith, language: Language) {
        *self.session() = Some(Session::new(recorded_with, language, script(language)));
    }

    pub fn recorded(&self, take: Take) {
        if let Some(session) = self.session().as_mut() {
            session.recorded(take);
        }
    }

    pub fn keep(&self, said: &str) -> bool {
        self.session()
            .as_mut()
            .is_some_and(|session| session.keep(said))
    }

    pub fn redo(&self) {
        if let Some(session) = self.session().as_mut() {
            session.redo();
        }
    }

    pub fn skip(&self) {
        if let Some(session) = self.session().as_mut() {
            session.skip();
        }
    }

    pub fn cancel(&self) {
        *self.session() = None;
    }

    /// Ends the session and writes what was kept as a set in a folder of its
    /// own. `seconds_since_1970` only makes the folder's name unique.
    pub fn finish(&self, seconds_since_1970: u64) -> Result<PathBuf, String> {
        let Some(session) = self.session().take() else {
            return Err("no session".to_string());
        };
        let set = session.finish();
        let folder = self
            .sets_folder
            .join(folder_name(&set.recorded_with.speaker, seconds_since_1970));
        buttervoice_samples::save(&folder, &set).map_err(|error| error.to_string())?;
        Ok(folder)
    }

    pub fn state(&self) -> State {
        let session = self.session();
        let Some(session) = session.as_ref() else {
            return State {
                active: false,
                position: 0,
                total: 0,
                prompt: None,
                take: None,
            };
        };
        let (position, total) = session.progress();
        State {
            active: true,
            position,
            total,
            prompt: session.prompt().map(|prompt| PromptState {
                kind: prompt.kind.name(),
                say: prompt.say.clone(),
                instruction: prompt.instruction.clone(),
                conditions: prompt.conditions.clone(),
            }),
            take: session.take().map(|take| TakeState {
                heard: take.heard.clone(),
                seconds: take.audio.duration().as_secs_f64(),
            }),
        }
    }

    fn session(&self) -> MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The speaker's label is typed by a person and becomes part of a path, so
/// only letters, digits, `-` and `_` are let through.
fn folder_name(speaker: &str, seconds_since_1970: u64) -> String {
    let safe: String = speaker
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let label = if safe.is_empty() { "speaker" } else { &safe };
    format!("{label}-{seconds_since_1970}")
}

#[cfg(test)]
mod tests {
    use super::folder_name;

    #[test]
    fn a_label_cannot_steer_where_a_set_is_written() {
        assert_eq!(folder_name("../../etc/passwd", 17), "------etc-passwd-17");
        assert_eq!(folder_name("Ana Lúcia", 17), "Ana-L-cia-17");
    }

    #[test]
    fn a_plain_label_is_kept_and_an_empty_one_gets_a_name() {
        assert_eq!(folder_name("speaker_a-1", 17), "speaker_a-1-17");
        assert_eq!(folder_name("   ", 17), "speaker-17");
    }
}
