//! The rules of a recording session: one prompt at a time, each take
//! confirmed or redone before it counts.

use buttervoice_core::capture::{CapturedAudio, InputKind};
use buttervoice_core::language::Language;

use std::path::Path;

use crate::{
    Kind, RecordedWith, Sample, SampleSet, SaveError, replace, write_description, write_wav,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub kind: Kind,
    /// The words to say, when the prompt gives them.
    pub say: Option<String>,
    /// What to do, when it is more than reading the words out.
    pub instruction: Option<String>,
    pub conditions: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Take {
    pub audio: CapturedAudio,
    pub input: InputKind,
    /// What a transcriber made of it, if one ran. Only a starting point for
    /// the person to correct.
    pub heard: Option<String>,
}

#[derive(Debug)]
pub struct Session {
    recorded_with: RecordedWith,
    language: Language,
    prompts: Vec<Prompt>,
    position: usize,
    take: Option<Take>,
    /// One slot per prompt, so going back and recording again replaces a
    /// sample instead of adding a second one.
    kept: Vec<Option<Sample>>,
}

/// Each prompt's take is saved under the prompt's own number, so skipping
/// one or recording one again never renames the others.
fn slot_audio(slot: usize) -> String {
    format!("{:04}.wav", slot + 1)
}

impl Session {
    pub fn new(recorded_with: RecordedWith, language: Language, prompts: Vec<Prompt>) -> Self {
        Self {
            kept: vec![None; prompts.len()],
            recorded_with,
            language,
            prompts,
            position: 0,
            take: None,
        }
    }

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompts.get(self.position)
    }

    pub fn take(&self) -> Option<&Take> {
        self.take.as_ref()
    }

    /// How many prompts are behind the person, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.position, self.prompts.len())
    }

    /// A new recording replaces one that has not been kept yet. Once the
    /// prompts have run out there is nothing for it to answer.
    pub fn recorded(&mut self, take: Take) {
        if self.prompt().is_some() {
            self.take = Some(take);
        }
    }

    /// Keeps the take as a sample of what the person says they said.
    /// Returns false when there is no take to keep.
    pub fn keep(&mut self, said: &str) -> bool {
        let (Some(take), Some(prompt)) = (&self.take, self.prompts.get(self.position)) else {
            return false;
        };
        self.kept[self.position] = Some(self.sample_from(take, prompt, said));
        self.take = None;
        self.position += 1;
        true
    }

    fn sample_from(&self, take: &Take, prompt: &Prompt, said: &str) -> Sample {
        Sample {
            kind: prompt.kind,
            language: self.language,
            said: said.to_string(),
            prompt: prompt.say.clone(),
            conditions: prompt.conditions.clone(),
            input: take.input,
            audio: take.audio.clone(),
        }
    }

    /// Moves on, keeping a take that is waiting as what the person says
    /// they said. With no take waiting it moves on keeping nothing new.
    pub fn next(&mut self, said: &str) {
        if !self.keep(said) {
            self.skip();
        }
    }

    /// Keeps the take waiting as what the person says they said, saving it
    /// into `folder` first. Nothing changes unless it was saved. Returns
    /// false when there is no take to keep.
    pub fn keep_into(&mut self, said: &str, folder: &Path) -> Result<bool, SaveError> {
        let (Some(take), Some(prompt)) = (&self.take, self.prompts.get(self.position)) else {
            return Ok(false);
        };
        let sample = self.sample_from(take, prompt, said);
        std::fs::create_dir_all(folder)?;
        let audio = folder.join(slot_audio(self.position));
        replace(&audio, |partial| write_wav(partial, &sample.audio))?;
        let listing = self.kept.iter().enumerate().filter_map(|(slot, kept)| {
            let sample = if slot == self.position {
                Some(&sample)
            } else {
                kept.as_ref()
            };
            sample.map(|sample| (slot_audio(slot), sample))
        });
        write_description(folder, &self.recorded_with, listing)?;
        self.kept[self.position] = Some(sample);
        self.take = None;
        self.position += 1;
        Ok(true)
    }

    /// Saves everything kept so far into `folder`, writing whatever it is
    /// missing.
    pub fn save_into(&self, folder: &Path) -> Result<(), SaveError> {
        std::fs::create_dir_all(folder)?;
        let kept: Vec<(String, &Sample)> = self
            .kept
            .iter()
            .enumerate()
            .filter_map(|(slot, kept)| Some((slot_audio(slot), kept.as_ref()?)))
            .collect();
        for (audio, sample) in &kept {
            let path = folder.join(audio);
            if !path.exists() {
                replace(&path, |partial| write_wav(partial, &sample.audio))?;
            }
        }
        write_description(folder, &self.recorded_with, kept)?;
        Ok(())
    }

    pub fn skip(&mut self) {
        self.take = None;
        if self.prompt().is_some() {
            self.position += 1;
        }
    }

    /// Returns to the previous prompt. A take not yet kept is discarded;
    /// what was kept there stays until something replaces it.
    pub fn back(&mut self) {
        self.take = None;
        self.position = self.position.saturating_sub(1);
    }

    /// The sample already kept for this prompt, from an earlier visit.
    pub fn kept_here(&self) -> Option<&Sample> {
        self.kept.get(self.position)?.as_ref()
    }

    pub fn kept_count(&self) -> usize {
        self.kept.iter().flatten().count()
    }

    pub fn finish(self) -> SampleSet {
        SampleSet {
            recorded_with: self.recorded_with,
            samples: self.kept.into_iter().flatten().collect(),
        }
    }
}
