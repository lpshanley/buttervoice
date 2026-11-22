pub mod diff;
pub mod package;
pub mod script;
pub mod session;

use std::path::{Path, PathBuf};

use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use serde::{Deserialize, Serialize};

/// Raised whenever a set written by this version could not be read by the
/// one before it.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    ReadSentence,
    ShortUtterance,
    LongDictation,
    HardWords,
    NaturalSpeech,
    /// The same words under a condition worth isolating: a quiet voice,
    /// distance from the microphone, background noise.
    Conditions,
    /// The key held with nothing said.
    Silence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub kind: Kind,
    pub language: Language,
    /// What the person actually said, which is what a transcript is scored
    /// against. Not always what they were asked to say.
    pub said: String,
    pub prompt: Option<String>,
    pub conditions: Option<String>,
    pub input: InputKind,
    pub audio: CapturedAudio,
}

/// Where and by whom a set was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedWith {
    pub speaker: String,
    pub app_version: String,
    pub app_commit: String,
    pub chip: String,
    pub macos: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleSet {
    pub recorded_with: RecordedWith,
    pub samples: Vec<Sample>,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("the set could not be written")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum LoadError {
    #[error("this folder is not a sample set")]
    NotASet,
    #[error("the set was written in format version {0}, which this version cannot read")]
    UnknownVersion(u32),
    #[error("the set's description could not be understood: {0}")]
    Unreadable(String),
    #[error("the audio file {0} is missing or unreadable")]
    MissingAudio(String),
}

pub(crate) const DESCRIPTION: &str = "set.json";

#[derive(Serialize, Deserialize)]
struct SetFile {
    format_version: u32,
    recorded_with: RecordedWithFile,
    samples: Vec<SampleFile>,
}

#[derive(Serialize, Deserialize)]
struct RecordedWithFile {
    speaker: String,
    app_version: String,
    app_commit: String,
    chip: String,
    macos: String,
}

#[derive(Serialize, Deserialize)]
struct SampleFile {
    audio: String,
    kind: String,
    language: String,
    said: String,
    prompt: Option<String>,
    conditions: Option<String>,
    input: String,
    dropped_samples: u64,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::ReadSentence,
        Kind::ShortUtterance,
        Kind::LongDictation,
        Kind::HardWords,
        Kind::NaturalSpeech,
        Kind::Conditions,
        Kind::Silence,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::ReadSentence => "read_sentence",
            Kind::ShortUtterance => "short_utterance",
            Kind::LongDictation => "long_dictation",
            Kind::HardWords => "hard_words",
            Kind::NaturalSpeech => "natural_speech",
            Kind::Conditions => "conditions",
            Kind::Silence => "silence",
        }
    }
}

fn input_name(input: InputKind) -> &'static str {
    match input {
        InputKind::BuiltIn => "built_in",
        InputKind::Other => "other",
    }
}

fn unknown(what: &str, name: &str) -> LoadError {
    LoadError::Unreadable(format!("unknown {what} {name:?}"))
}

pub fn save(folder: &Path, set: &SampleSet) -> Result<(), SaveError> {
    std::fs::create_dir_all(folder)?;
    let mut listing = Vec::with_capacity(set.samples.len());
    for (index, sample) in set.samples.iter().enumerate() {
        let audio = format!("{:04}.wav", index + 1);
        write_wav(&folder.join(&audio), &sample.audio)?;
        listing.push((audio, sample));
    }
    write_description(folder, &set.recorded_with, listing)?;
    Ok(())
}

/// Writes the set's description, replacing the one there only once the
/// new one is whole.
pub(crate) fn write_description<'a>(
    folder: &Path,
    recorded: &RecordedWith,
    samples: impl IntoIterator<Item = (String, &'a Sample)>,
) -> std::io::Result<()> {
    let samples = samples
        .into_iter()
        .map(|(audio, sample)| SampleFile {
            audio,
            kind: sample.kind.name().to_string(),
            language: sample.language.tag().to_string(),
            said: sample.said.clone(),
            prompt: sample.prompt.clone(),
            conditions: sample.conditions.clone(),
            input: input_name(sample.input).to_string(),
            dropped_samples: sample.audio.dropped_samples,
        })
        .collect();
    let file = SetFile {
        format_version: FORMAT_VERSION,
        recorded_with: RecordedWithFile {
            speaker: recorded.speaker.clone(),
            app_version: recorded.app_version.clone(),
            app_commit: recorded.app_commit.clone(),
            chip: recorded.chip.clone(),
            macos: recorded.macos.clone(),
        },
        samples,
    };
    let description = serde_json::to_string(&file).map_err(std::io::Error::other)?;
    replace(&folder.join(DESCRIPTION), |partial| {
        std::fs::write(partial, &description)
    })
}

/// Writes `path` by way of a file beside it, so a write cut short never
/// leaves half a file where a whole one was.
pub(crate) fn replace(
    path: &Path,
    write: impl FnOnce(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    write(&partial)?;
    std::fs::rename(&partial, path)
}

/// A set in a suite, by the name of its folder.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteSet {
    pub name: String,
    pub set: SampleSet,
}

#[derive(Debug, thiserror::Error)]
pub enum SuiteError {
    #[error("the folder could not be read")]
    Unreadable(#[from] std::io::Error),
    #[error("the set {name} could not be loaded: {error}")]
    Set { name: String, error: LoadError },
    #[error("there are no sample sets in this folder")]
    NoSets,
}

/// Every set in `folder`, by name, or the set `folder` is.
pub fn load_suite(folder: &Path) -> Result<Vec<SuiteSet>, SuiteError> {
    let named = |folder: &Path| {
        folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    let mut folders = if folder.join(DESCRIPTION).is_file() {
        vec![folder.to_path_buf()]
    } else {
        let mut folders = Vec::new();
        for entry in std::fs::read_dir(folder)? {
            let path = entry?.path();
            if path.join(DESCRIPTION).is_file() {
                folders.push(path);
            }
        }
        folders
    };
    folders.sort();
    let sets: Vec<SuiteSet> = folders
        .iter()
        .map(|folder| {
            let name = named(folder).unwrap_or_default();
            match load(folder) {
                Ok(set) => Ok(SuiteSet { name, set }),
                Err(error) => Err(SuiteError::Set { name, error }),
            }
        })
        .collect::<Result<_, _>>()?;
    if sets.is_empty() {
        return Err(SuiteError::NoSets);
    }
    Ok(sets)
}

pub fn load(folder: &Path) -> Result<SampleSet, LoadError> {
    let description =
        std::fs::read_to_string(folder.join(DESCRIPTION)).map_err(|_| LoadError::NotASet)?;
    let unreadable = |error: serde_json::Error| LoadError::Unreadable(error.to_string());
    let value: serde_json::Value = serde_json::from_str(&description).map_err(unreadable)?;
    // The version is read by itself first: a newer set may not fit this
    // version's idea of a set at all.
    let version = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(LoadError::NotASet)?;
    if version != u64::from(FORMAT_VERSION) {
        return Err(LoadError::UnknownVersion(version as u32));
    }
    let file: SetFile = serde_json::from_value(value).map_err(unreadable)?;
    let samples = file
        .samples
        .into_iter()
        .map(|sample| {
            let audio = read_wav(&folder.join(&sample.audio), sample.dropped_samples)
                .ok_or_else(|| LoadError::MissingAudio(sample.audio.clone()))?;
            Ok(Sample {
                kind: Kind::ALL
                    .into_iter()
                    .find(|kind| kind.name() == sample.kind)
                    .ok_or_else(|| unknown("kind", &sample.kind))?,
                language: Language::from_setting(Some(&sample.language))
                    .ok_or_else(|| unknown("language", &sample.language))?,
                said: sample.said,
                prompt: sample.prompt,
                conditions: sample.conditions,
                input: [InputKind::BuiltIn, InputKind::Other]
                    .into_iter()
                    .find(|input| input_name(*input) == sample.input)
                    .ok_or_else(|| unknown("input", &sample.input))?,
                audio,
            })
        })
        .collect::<Result<Vec<_>, LoadError>>()?;
    let recorded = file.recorded_with;
    Ok(SampleSet {
        recorded_with: RecordedWith {
            speaker: recorded.speaker,
            app_version: recorded.app_version,
            app_commit: recorded.app_commit,
            chip: recorded.chip,
            macos: recorded.macos,
        },
        samples,
    })
}

/// 32-bit float, exactly as captured: converting to 16-bit would throw away
/// the quiet detail that separates one microphone from another.
pub(crate) fn write_wav(path: &Path, audio: &CapturedAudio) -> std::io::Result<()> {
    let spec = hound::WavSpec {
        channels: audio.format.channels,
        sample_rate: audio.format.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(std::io::Error::other)?;
    for sample in &audio.samples {
        writer
            .write_sample(*sample)
            .map_err(std::io::Error::other)?;
    }
    writer.finalize().map_err(std::io::Error::other)
}

fn read_wav(path: &Path, dropped_samples: u64) -> Option<CapturedAudio> {
    let mut reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(CapturedAudio {
        format: AudioFormat {
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        },
        samples,
        dropped_samples,
    })
}
