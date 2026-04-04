use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use webrtc_vad::{SampleRate, Vad, VadMode};

const VAD_FRAME_MS: u32 = 30;
const VAD_LEADING_PADDING_MS: u32 = 300;
const VAD_TRAILING_PADDING_MS: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreprocessStatus {
    Trimmed,
    UsedRawNoChange,
    UsedRawNoSpeech,
    SkippedUnsupportedFormat,
    SkippedUnsupportedSampleRate,
}

impl PreprocessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trimmed => "trimmed",
            Self::UsedRawNoChange => "used_raw_no_change",
            Self::UsedRawNoSpeech => "used_raw_no_speech",
            Self::SkippedUnsupportedFormat => "skipped_unsupported_format",
            Self::SkippedUnsupportedSampleRate => "skipped_unsupported_sample_rate",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreprocessOutcome {
    pub audio_path: PathBuf,
    pub cleanup_path: Option<PathBuf>,
    pub duration_ms: u64,
    pub raw_duration_ms: u64,
    pub output_duration_ms: u64,
    pub status: PreprocessStatus,
}

pub fn preprocess_wav_with_vad(
    audio_path: &Path,
    scratch_dir: &Path,
    request_id: &str,
) -> Result<PreprocessOutcome> {
    let start = Instant::now();
    let reader = hound::WavReader::open(audio_path).with_context(|| {
        format!(
            "failed opening {} for VAD preprocessing",
            audio_path.display()
        )
    })?;
    let spec = reader.spec();

    let samples = if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        Vec::new()
    } else {
        reader
            .into_samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed reading samples for VAD preprocessing")?
    };

    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Ok(PreprocessOutcome {
            audio_path: audio_path.to_path_buf(),
            cleanup_path: None,
            duration_ms: start.elapsed().as_millis() as u64,
            raw_duration_ms: wav_duration_ms(spec.sample_rate, 0),
            output_duration_ms: wav_duration_ms(spec.sample_rate, 0),
            status: PreprocessStatus::SkippedUnsupportedFormat,
        });
    }

    let raw_duration_ms = wav_duration_ms(spec.sample_rate, samples.len());
    let sample_rate = match map_sample_rate(spec.sample_rate) {
        Some(rate) => rate,
        None => {
            return Ok(PreprocessOutcome {
                audio_path: audio_path.to_path_buf(),
                cleanup_path: None,
                duration_ms: start.elapsed().as_millis() as u64,
                raw_duration_ms,
                output_duration_ms: raw_duration_ms,
                status: PreprocessStatus::SkippedUnsupportedSampleRate,
            })
        }
    };

    let frame_samples = ((spec.sample_rate as usize) * (VAD_FRAME_MS as usize)) / 1000;
    if frame_samples == 0 || samples.is_empty() {
        return Ok(PreprocessOutcome {
            audio_path: audio_path.to_path_buf(),
            cleanup_path: None,
            duration_ms: start.elapsed().as_millis() as u64,
            raw_duration_ms,
            output_duration_ms: raw_duration_ms,
            status: PreprocessStatus::UsedRawNoSpeech,
        });
    }

    let mut vad = Vad::new_with_rate_and_mode(sample_rate, VadMode::Aggressive);
    let mut first_voice_idx = None;
    let mut last_voice_idx = None;

    for (frame_idx, chunk) in samples.chunks(frame_samples).enumerate() {
        let mut frame = vec![0_i16; frame_samples];
        frame[..chunk.len()].copy_from_slice(chunk);
        let voiced = vad.is_voice_segment(&frame).unwrap_or(false);
        if voiced {
            first_voice_idx.get_or_insert(frame_idx);
            last_voice_idx = Some(frame_idx);
        }
    }

    let (first_voice_idx, last_voice_idx) = match (first_voice_idx, last_voice_idx) {
        (Some(first), Some(last)) => (first, last),
        _ => {
            return Ok(PreprocessOutcome {
                audio_path: audio_path.to_path_buf(),
                cleanup_path: None,
                duration_ms: start.elapsed().as_millis() as u64,
                raw_duration_ms,
                output_duration_ms: raw_duration_ms,
                status: PreprocessStatus::UsedRawNoSpeech,
            })
        }
    };

    let speech_start = first_voice_idx.saturating_mul(frame_samples);
    let speech_end = ((last_voice_idx + 1).saturating_mul(frame_samples)).min(samples.len());
    let leading_padding_samples =
        ((spec.sample_rate as usize) * (VAD_LEADING_PADDING_MS as usize)) / 1000;
    let trailing_padding_samples =
        ((spec.sample_rate as usize) * (VAD_TRAILING_PADDING_MS as usize)) / 1000;
    let trim_start = speech_start.saturating_sub(leading_padding_samples);
    let trim_end = speech_end
        .saturating_add(trailing_padding_samples)
        .min(samples.len());

    if trim_start == 0 && trim_end >= samples.len() {
        return Ok(PreprocessOutcome {
            audio_path: audio_path.to_path_buf(),
            cleanup_path: None,
            duration_ms: start.elapsed().as_millis() as u64,
            raw_duration_ms,
            output_duration_ms: raw_duration_ms,
            status: PreprocessStatus::UsedRawNoChange,
        });
    }

    fs::create_dir_all(scratch_dir)
        .with_context(|| format!("failed creating VAD scratch dir {}", scratch_dir.display()))?;
    let trimmed_path = scratch_dir.join(format!(
        "{}-vad-{}.wav",
        sanitize_request_id(request_id),
        unique_suffix(),
    ));
    let mut writer = hound::WavWriter::create(&trimmed_path, spec)
        .with_context(|| format!("failed creating trimmed VAD wav {}", trimmed_path.display()))?;
    for sample in &samples[trim_start..trim_end] {
        writer
            .write_sample(*sample)
            .context("failed writing trimmed VAD sample")?;
    }
    writer
        .finalize()
        .context("failed finalizing trimmed VAD wav")?;

    let output_duration_ms = wav_duration_ms(spec.sample_rate, trim_end.saturating_sub(trim_start));
    Ok(PreprocessOutcome {
        audio_path: trimmed_path.clone(),
        cleanup_path: Some(trimmed_path),
        duration_ms: start.elapsed().as_millis() as u64,
        raw_duration_ms,
        output_duration_ms,
        status: PreprocessStatus::Trimmed,
    })
}

fn map_sample_rate(sample_rate: u32) -> Option<SampleRate> {
    match sample_rate {
        8_000 => Some(SampleRate::Rate8kHz),
        16_000 => Some(SampleRate::Rate16kHz),
        32_000 => Some(SampleRate::Rate32kHz),
        48_000 => Some(SampleRate::Rate48kHz),
        _ => None,
    }
}

fn wav_duration_ms(sample_rate_hz: u32, sample_count: usize) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    ((sample_count as u64) * 1000) / sample_rate_hz as u64
}

fn sanitize_request_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_wav_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("buttervoice-{label}-{}.wav", unique_suffix()))
    }

    fn write_wav(path: &Path, sample_rate: u32, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for sample in samples {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn append_constant(samples: &mut Vec<i16>, sample_rate: u32, duration_ms: u32, value: i16) {
        let count = ((sample_rate as usize) * (duration_ms as usize)) / 1000;
        samples.extend(std::iter::repeat_n(value, count));
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let wav = temp_wav_path("vad-trim");
        let scratch_dir = std::env::temp_dir().join(format!("buttervoice-vad-{}", unique_suffix()));
        let mut samples = Vec::new();
        append_constant(&mut samples, 16_000, 600, 0);
        append_constant(&mut samples, 16_000, 1_000, 5_000);
        append_constant(&mut samples, 16_000, 900, 0);
        write_wav(&wav, 16_000, &samples);

        let outcome = preprocess_wav_with_vad(&wav, &scratch_dir, "trim-test").unwrap();

        assert_eq!(outcome.status, PreprocessStatus::Trimmed);
        assert!(outcome.output_duration_ms < outcome.raw_duration_ms);
        assert!(outcome.cleanup_path.is_some());

        let _ = fs::remove_file(wav);
        if let Some(path) = outcome.cleanup_path {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir_all(scratch_dir);
    }

    #[test]
    fn preserves_interior_pauses() {
        let wav = temp_wav_path("vad-interior");
        let scratch_dir = std::env::temp_dir().join(format!("buttervoice-vad-{}", unique_suffix()));
        let mut samples = Vec::new();
        append_constant(&mut samples, 16_000, 400, 0);
        append_constant(&mut samples, 16_000, 450, 6_000);
        append_constant(&mut samples, 16_000, 300, 0);
        append_constant(&mut samples, 16_000, 450, 6_000);
        append_constant(&mut samples, 16_000, 650, 0);
        write_wav(&wav, 16_000, &samples);

        let outcome = preprocess_wav_with_vad(&wav, &scratch_dir, "interior-test").unwrap();

        assert_eq!(outcome.status, PreprocessStatus::Trimmed);
        assert!(outcome.output_duration_ms >= 1_400);

        let _ = fs::remove_file(wav);
        if let Some(path) = outcome.cleanup_path {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir_all(scratch_dir);
    }

    #[test]
    fn falls_back_to_raw_when_no_speech_detected() {
        let wav = temp_wav_path("vad-silence");
        let scratch_dir = std::env::temp_dir().join(format!("buttervoice-vad-{}", unique_suffix()));
        let samples = vec![0_i16; 16_000];
        write_wav(&wav, 16_000, &samples);

        let outcome = preprocess_wav_with_vad(&wav, &scratch_dir, "silence-test").unwrap();

        assert_eq!(outcome.status, PreprocessStatus::UsedRawNoSpeech);
        assert_eq!(outcome.audio_path, wav);
        assert!(outcome.cleanup_path.is_none());

        let _ = fs::remove_file(&outcome.audio_path);
        let _ = fs::remove_dir_all(scratch_dir);
    }

    #[test]
    fn skips_unsupported_sample_rate() {
        let wav = temp_wav_path("vad-rate");
        let scratch_dir = std::env::temp_dir().join(format!("buttervoice-vad-{}", unique_suffix()));
        let samples = vec![0_i16; 22_050];
        write_wav(&wav, 22_050, &samples);

        let outcome = preprocess_wav_with_vad(&wav, &scratch_dir, "rate-test").unwrap();

        assert_eq!(
            outcome.status,
            PreprocessStatus::SkippedUnsupportedSampleRate
        );
        assert_eq!(outcome.audio_path, wav);
        assert!(outcome.cleanup_path.is_none());

        let _ = fs::remove_file(&outcome.audio_path);
        let _ = fs::remove_dir_all(scratch_dir);
    }
}
