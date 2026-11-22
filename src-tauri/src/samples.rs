//! The recording session a person is walking through, if any, and where
//! finished sets are kept.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use buttervoice_core::key_dictation::Purpose;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::Provider;
use buttervoice_samples::diff::{Part, word_diff};
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
    /// Where what the model heard differs from the prompt's words. Empty
    /// when there are no words to compare, as for free speech.
    difference: Vec<PartState>,
}

#[derive(serde::Serialize)]
pub struct PartState {
    kind: &'static str,
    said: Option<String>,
    heard: Option<String>,
}

impl From<Part> for PartState {
    fn from(part: Part) -> Self {
        let (kind, said, heard) = match part {
            Part::Same(word) => ("same", Some(word), None),
            Part::Missing(word) => ("missing", Some(word), None),
            Part::Extra(word) => ("extra", None, Some(word)),
            Part::Changed { said, heard } => ("changed", Some(said), Some(heard)),
        };
        PartState { kind, said, heard }
    }
}

#[derive(serde::Serialize)]
pub struct KeptState {
    said: String,
    seconds: f64,
}

#[derive(serde::Serialize)]
pub struct State {
    active: bool,
    position: usize,
    total: usize,
    prompt: Option<PromptState>,
    take: Option<TakeState>,
    /// What was kept for this prompt on an earlier visit.
    kept_here: Option<KeptState>,
    kept: usize,
}

pub struct Recorder {
    sets_folder: PathBuf,
    /// The session and the folder it is saved to as it goes.
    session: Mutex<Option<(Session, PathBuf)>>,
    last_saved: Mutex<Option<PathBuf>>,
    window_shown: AtomicBool,
}

impl Recorder {
    pub fn new(sets_folder: PathBuf) -> Self {
        Self {
            sets_folder,
            session: Mutex::new(None),
            last_saved: Mutex::new(None),
            window_shown: AtomicBool::new(false),
        }
    }

    pub fn active(&self) -> bool {
        self.session().is_some()
    }

    /// The window being open or closed. A dictation is a take only while
    /// the person can see the prompt it answers.
    pub fn window_shown(&self, shown: bool) {
        self.window_shown.store(shown, Ordering::Relaxed);
    }

    /// What a dictation begun now is for, given the provider chosen for
    /// ordinary dictation.
    pub fn purpose_for(&self, provider: Provider) -> Purpose {
        if self.window_shown.load(Ordering::Relaxed) && self.active() {
            Purpose::SampleTake
        } else {
            Purpose::Dictation(provider)
        }
    }

    /// `seconds_since_1970` only makes the folder's name unique.
    pub fn start(&self, recorded_with: RecordedWith, language: Language, seconds_since_1970: u64) {
        let folder = self
            .sets_folder
            .join(folder_name(&recorded_with.speaker, seconds_since_1970));
        *self.session() = Some((
            Session::new(recorded_with, language, script(language)),
            folder,
        ));
    }

    pub fn recorded(&self, take: Take) {
        if let Some((session, _)) = self.session().as_mut() {
            session.recorded(take);
        }
    }

    /// Every kept take is saved straight away, so quitting part-way loses
    /// nothing that was kept.
    pub fn next(&self, said: &str) {
        if let Some((session, folder)) = self.session().as_mut() {
            session.next(said);
            let so_far = session.so_far();
            if !so_far.samples.is_empty()
                && let Err(error) = buttervoice_samples::save(folder, &so_far)
            {
                tracing::warn!(%error, "the samples kept so far could not be saved");
            }
        }
    }

    pub fn skip(&self) {
        if let Some((session, _)) = self.session().as_mut() {
            session.skip();
        }
    }

    pub fn back(&self) {
        if let Some((session, _)) = self.session().as_mut() {
            session.back();
        }
    }

    /// The take waiting to be kept, or else what was kept for this prompt,
    /// as a WAV file to play back.
    pub fn audio(&self) -> Option<Vec<u8>> {
        let session = self.session();
        let (session, _) = session.as_ref()?;
        let audio = match session.take() {
            Some(take) => &take.audio,
            None => &session.kept_here()?.audio,
        };
        Some(audio.to_wav())
    }

    /// Stops and removes what this session saved.
    pub fn cancel(&self) {
        if let Some((_, folder)) = self.session().take() {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// Ends the session, writes what was kept, and returns its folder.
    pub fn finish(&self) -> Result<PathBuf, String> {
        let Some((session, folder)) = self.session().take() else {
            return Err("no session".to_string());
        };
        buttervoice_samples::save(&folder, &session.finish()).map_err(|error| error.to_string())?;
        *self
            .last_saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(folder.clone());
        Ok(folder)
    }

    /// Packs the set saved last into one file in `into` and returns it.
    /// Only that set can be packed: the window cannot name another folder.
    pub fn package_last_saved(&self, into: &std::path::Path) -> Result<PathBuf, String> {
        let saved = self
            .last_saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or("no set has been saved")?;
        let name = saved
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let package = into.join(format!("ButterVoice samples {name}.zip"));
        buttervoice_samples::package::pack(&saved, &package).map_err(|error| error.to_string())?;
        Ok(package)
    }

    pub fn state(&self) -> State {
        let session = self.session();
        let Some((session, _)) = session.as_ref() else {
            return State {
                active: false,
                position: 0,
                total: 0,
                prompt: None,
                take: None,
                kept_here: None,
                kept: 0,
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
                difference: match (
                    session.prompt().and_then(|prompt| prompt.say.as_deref()),
                    &take.heard,
                ) {
                    (Some(said), Some(heard)) => word_diff(said, heard)
                        .into_iter()
                        .map(PartState::from)
                        .collect(),
                    _ => Vec::new(),
                },
            }),
            kept_here: session.kept_here().map(|sample| KeptState {
                said: sample.said.clone(),
                seconds: sample.audio.duration().as_secs_f64(),
            }),
            kept: session.kept_count(),
        }
    }

    fn session(&self) -> MutexGuard<'_, Option<(Session, PathBuf)>> {
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
    use super::{Recorder, folder_name};
    use buttervoice_core::key_dictation::Purpose;
    use buttervoice_core::language::Language;
    use buttervoice_core::transcription::Provider;
    use buttervoice_samples::RecordedWith;

    fn a_session_started(recorder: &Recorder) {
        let recorded_with = RecordedWith {
            speaker: "speaker-a".to_string(),
            app_version: "0.2.0".to_string(),
            app_commit: "0a1b2c3".to_string(),
            chip: "Apple M5 Pro".to_string(),
            macos: "27.2".to_string(),
        };
        recorder.start(recorded_with, Language::English, 17);
    }

    #[test]
    fn a_dictation_is_a_take_only_while_a_session_is_on_screen() {
        let recorder = Recorder::new("unused".into());
        let dictation = Purpose::Dictation(Provider::Grok);
        assert_eq!(recorder.purpose_for(Provider::Grok), dictation);

        recorder.window_shown(true);
        assert_eq!(recorder.purpose_for(Provider::Grok), dictation);

        a_session_started(&recorder);
        assert_eq!(recorder.purpose_for(Provider::Grok), Purpose::SampleTake);

        recorder.window_shown(false);
        assert_eq!(recorder.purpose_for(Provider::Grok), dictation);
    }

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
