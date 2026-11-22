//! Runs saved recordings through transcription in several variants and
//! scores each against the sentence that was read.
//!
//!     cargo run --release -p buttervoice-replay -- replay/recordings replay/sentences.txt [model.gguf ...]
//!
//! Each model file is run on this machine. The Grok variants run only when
//! `XAI_API_KEY` is set; without it nothing leaves the machine.

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use buttervoice_grok::{DEFAULT_BASE_URL, GrokTranscriber};
use buttervoice_local::LocalTranscriber;
use buttervoice_replay::recordings::{self, Recording};
use buttervoice_replay::report::{Report, replay};

struct GrokVariant {
    name: &'static str,
    /// Boosted the way the app boosts audio before transcription.
    boosted: bool,
    vad_threshold: Option<f32>,
}

const GROK_VARIANTS: [GrokVariant; 4] = [
    GrokVariant {
        name: "grok, as captured",
        boosted: false,
        vad_threshold: None,
    },
    GrokVariant {
        name: "grok, boosted",
        boosted: true,
        vad_threshold: None,
    },
    GrokVariant {
        name: "grok, gate 0.1",
        boosted: false,
        vad_threshold: Some(0.1),
    },
    GrokVariant {
        name: "grok, boosted, gate 0.1",
        boosted: true,
        vad_threshold: Some(0.1),
    },
];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

struct Run<'a> {
    recordings: &'a [Recording],
    references: &'a [String],
    report_file: std::fs::File,
}

impl Run<'_> {
    fn variant(
        &mut self,
        name: &str,
        transcribe: impl Fn(&CapturedAudio) -> Result<Transcript, TranscriptionFailure>,
    ) -> Result<(), String> {
        let took = RefCell::new(Vec::new());
        let report = replay(self.recordings, self.references, |audio| {
            let started = Instant::now();
            let outcome = transcribe(audio);
            took.borrow_mut().push(started.elapsed());
            outcome
        });
        let took = took.into_inner();
        println!(
            "{:<44} {:>9.1}% {:>7} {:>9} {:>9.0} {:>10.0}",
            name,
            report.mean_word_error_rate() * 100.0,
            report.blanks(),
            report.failures(),
            took.first().copied().unwrap_or_default().as_secs_f64() * 1000.0,
            median(took).as_secs_f64() * 1000.0
        );
        write_lines(&mut self.report_file, name, &report)
            .map_err(|error| format!("could not write the report: {error}"))
    }
}

fn median(mut durations: Vec<Duration>) -> Duration {
    durations.sort();
    durations
        .get(durations.len() / 2)
        .copied()
        .unwrap_or_default()
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(recordings_dir), Some(sentences_file)) = (args.next(), args.next()) else {
        return Err(
            "usage: buttervoice-replay <recordings folder> <sentences file> [model.gguf ...]"
                .into(),
        );
    };
    let models: Vec<String> = args.collect();
    let api_key = std::env::var("XAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty());
    if api_key.is_none() && models.is_empty() {
        return Err("nothing to run: XAI_API_KEY is not set and no model file was given".into());
    }

    let recordings = recordings::load_all(Path::new(&recordings_dir))
        .map_err(|error| format!("could not read recordings from {recordings_dir}: {error}"))?;
    let sentences: Vec<String> = std::fs::read_to_string(&sentences_file)
        .map_err(|error| format!("could not read {sentences_file}: {error}"))?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if recordings.is_empty() {
        return Err(format!("there are no recordings in {recordings_dir}"));
    }
    if recordings.len() > sentences.len() {
        return Err(format!(
            "there are {} recordings but only {} sentences",
            recordings.len(),
            sentences.len()
        ));
    }
    let references = &sentences[..recordings.len()];
    eprintln!(
        "{} recordings, scored against the first {} sentences",
        recordings.len(),
        references.len()
    );

    let report_path = Path::new(&recordings_dir).join("report.jsonl");
    let report_file = std::fs::File::create(&report_path)
        .map_err(|error| format!("could not create {}: {error}", report_path.display()))?;
    let mut run = Run {
        recordings: &recordings,
        references,
        report_file,
    };

    println!(
        "{:<44} {:>10} {:>7} {:>9} {:>9} {:>10}",
        "variant", "word errs", "blanks", "failures", "first ms", "median ms"
    );
    for model in &models {
        let name = Path::new(model)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| model.clone());
        let started = Instant::now();
        let transcriber = LocalTranscriber::load(Path::new(model))
            .map_err(|error| format!("{model}: {error}"))?;
        eprintln!(
            "{name}: loaded in {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let transcribe = |audio: &CapturedAudio| {
            transcriber.transcribe(audio).map_err(|error| {
                eprintln!("{name}: {error}");
                TranscriptionFailure::Rejected
            })
        };
        run.variant(&format!("{name}, as captured"), transcribe)?;
        run.variant(&format!("{name}, boosted"), |audio| {
            transcribe(&audio.boosted_for_transcription())
        })?;
    }
    match api_key {
        None => eprintln!("XAI_API_KEY is not set: the Grok variants were skipped"),
        Some(api_key) => {
            for variant in &GROK_VARIANTS {
                let mut transcriber = GrokTranscriber::new(DEFAULT_BASE_URL, api_key.clone())
                    .map_err(|error| error.to_string())?;
                if let Some(threshold) = variant.vad_threshold {
                    transcriber = transcriber.with_vad_threshold(threshold);
                }
                run.variant(variant.name, |audio| {
                    if variant.boosted {
                        transcriber.transcribe(&audio.boosted_for_transcription())
                    } else {
                        transcriber.transcribe(audio)
                    }
                })?;
            }
        }
    }
    eprintln!("Details written to {}", report_path.display());
    Ok(())
}

fn write_lines(file: &mut std::fs::File, variant: &str, report: &Report) -> std::io::Result<()> {
    for heard in &report.heard {
        let line = serde_json::json!({
            "variant": variant,
            "recording": heard.recording,
            "word_error_rate": heard.word_error_rate,
            "reference": heard.reference,
            "heard": heard.outcome.as_ref().ok(),
            "failure": heard.outcome.as_ref().err().map(|failure| format!("{failure:?}")),
        });
        writeln!(file, "{line}")?;
    }
    Ok(())
}
