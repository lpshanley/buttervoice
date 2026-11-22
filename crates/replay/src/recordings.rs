use std::path::{Path, PathBuf};

use buttervoice_core::capture::{AudioFormat, CapturedAudio};

#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub name: String,
    pub audio: CapturedAudio,
}

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Wav(#[from] hound::Error),
}

/// The number to give the next recording saved into `dir`.
pub fn next_number(dir: &Path) -> usize {
    wav_files(dir).map_or(1, |files| files.len() + 1)
}

/// Saved as 32-bit float, so the samples are exactly what was captured.
pub fn save(dir: &Path, number: usize, audio: &CapturedAudio) -> Result<PathBuf, RecordingError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{number:03}.wav"));
    let spec = hound::WavSpec {
        channels: audio.format.channels,
        sample_rate: audio.format.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&path, spec)?;
    for sample in &audio.samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(path)
}

/// Every recording in `dir`, in the order they were numbered.
pub fn load_all(dir: &Path) -> Result<Vec<Recording>, RecordingError> {
    wav_files(dir)?.into_iter().map(load).collect()
}

fn load(path: PathBuf) -> Result<Recording, RecordingError> {
    let mut reader = hound::WavReader::open(&path)?;
    let spec = reader.spec();
    let samples = reader.samples::<f32>().collect::<Result<Vec<f32>, _>>()?;
    Ok(Recording {
        name: path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default(),
        audio: CapturedAudio {
            format: AudioFormat {
                sample_rate: spec.sample_rate,
                channels: spec.channels,
            },
            samples,
            dropped_samples: 0,
        },
    })
}

fn wav_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "wav"))
        .collect();
    files.sort();
    Ok(files)
}
