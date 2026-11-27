//! Runs saved recordings through transcription in several variants and
//! scores each against the sentence that was read.
//!
//!     cargo run --release -p buttervoice-replay -- replay/recordings replay/sentences.txt [model.gguf ...]
//!     cargo run --release -p buttervoice-replay -- --suite <set or suite folder> <model.gguf ...>
//!     cargo run --release -p buttervoice-replay -- --import <package.zip> <suite folder>
//!
//! Each model file is run on this machine. The Grok variants run only when
//! `XAI_API_KEY` is set; without it nothing leaves the machine.

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use buttervoice_core::capture::CapturedAudio;
use buttervoice_core::capture::InputKind;
use buttervoice_core::language::Language;
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use buttervoice_grok::{DEFAULT_BASE_URL, FillerWords, GrokTranscriber};
use buttervoice_local::LocalTranscriber;
use buttervoice_replay::recordings::{self, Recording};
use buttervoice_replay::report::{Report, replay};
mod cost;

use buttervoice_replay::summary::{Scored, Summary, by, by_kind, summarise};

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
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("--import") {
        args.next();
        let (Some(package), Some(suite)) = (args.next(), args.next()) else {
            return Err("usage: buttervoice-replay --import <package.zip> <suite folder>".into());
        };
        let unpacked = buttervoice_samples::package::unpack(Path::new(&package), Path::new(&suite))
            .map_err(|error| format!("{package}: {error}"))?;
        let set = buttervoice_samples::load(&unpacked).map_err(|error| error.to_string())?;
        let by = &set.recorded_with;
        println!(
            "Added {} samples by {:?} ({} on macOS {}, app {}) to {}",
            set.samples.len(),
            by.speaker,
            by.chip,
            by.macos,
            by.app_version,
            unpacked.display()
        );
        return Ok(());
    }
    if args.peek().map(String::as_str) == Some(SCORE_ONE_MODEL) {
        args.next();
        let (Some(folder), Some(model)) = (args.next(), args.next()) else {
            return Err("usage: buttervoice-replay --score-one-model <folder> <model.gguf>".into());
        };
        return score_one_model(Path::new(&folder), &model);
    }
    if args.peek().map(String::as_str) == Some("--suite") {
        args.next();
        let folder = args
            .next()
            .ok_or("usage: buttervoice-replay --suite <set or suite folder> <model.gguf ...>")?;
        return run_suite(Path::new(&folder), &args.collect::<Vec<_>>());
    }
    let (Some(recordings_dir), Some(sentences_file)) = (args.next(), args.next()) else {
        return Err(
            "usage: buttervoice-replay <recordings folder> <sentences file> [model.gguf ...]"
                .into(),
        );
    };
    let models: Vec<String> = args.collect();
    buttervoice_local::silence_engine_log();
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
            transcriber
                .transcribe(audio, Language::English)
                .map_err(|error| {
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
                        transcriber.transcribe(
                            &audio.boosted_for_transcription(),
                            Language::English,
                            FillerWords::Dropped,
                        )
                    } else {
                        transcriber.transcribe(audio, Language::English, FillerWords::Dropped)
                    }
                })?;
            }
        }
    }
    eprintln!("Details written to {}", report_path.display());
    Ok(())
}

/// Scores each model on every labelled set in a suite: over all of it,
/// then by language, microphone and speaker where the suite has more than
/// one, then kind by kind. Only local models: a suite is never sent to a
/// cloud service by this tool.
fn run_suite(folder: &Path, models: &[String]) -> Result<(), String> {
    buttervoice_local::silence_engine_log();
    let suite = buttervoice_samples::load_suite(folder)
        .map_err(|error| format!("{}: {error}", folder.display()))?;
    if models.is_empty() {
        return Err("give at least one model file to score the suite with".into());
    }
    for set in &suite {
        let by = &set.set.recorded_with;
        eprintln!(
            "{}: {} samples by {:?} on {} (macOS {}), app {} {}",
            set.name,
            set.set.samples.len(),
            by.speaker,
            by.chip,
            by.macos,
            by.app_version,
            by.app_commit
        );
    }
    let report_path = folder.join("report.jsonl");
    // Started empty here; each model's run adds its lines.
    std::fs::File::create(&report_path)
        .map_err(|error| format!("could not create {}: {error}", report_path.display()))?;
    println!(
        "{:<24} {:<36} {:>7} {:>10} {:>9} {:>9} {:>12} {:>10}",
        "model",
        "samples of",
        "samples",
        "word errs",
        "invented",
        "failures",
        "x real time",
        "median ms"
    );
    // Each model is scored by a run of this tool of its own, so the memory
    // and energy it reports are that model's alone.
    let this_tool = std::env::current_exe().map_err(|error| error.to_string())?;
    for model in models {
        let status = std::process::Command::new(&this_tool)
            .args([SCORE_ONE_MODEL, &folder.display().to_string(), model])
            .status()
            .map_err(|error| format!("{model}: {error}"))?;
        if !status.success() {
            return Err(format!("{model}: scoring failed"));
        }
    }
    eprintln!("Details written to {}", report_path.display());
    Ok(())
}

/// Run by `--suite` once per model: scores that model alone, adding to the
/// report and printing its rows.
fn score_one_model(folder: &Path, model: &str) -> Result<(), String> {
    buttervoice_local::silence_engine_log();
    let suite = buttervoice_samples::load_suite(folder)
        .map_err(|error| format!("{}: {error}", folder.display()))?;
    let report_path = folder.join("report.jsonl");
    let mut report_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&report_path)
        .map_err(|error| format!("could not open {}: {error}", report_path.display()))?;
    let name = model_name(model);
    let started = Instant::now();
    let transcriber =
        LocalTranscriber::load(Path::new(model)).map_err(|error| format!("{model}: {error}"))?;
    eprintln!(
        "{name}: loaded in {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let mut scored = Vec::new();
    for set in &suite {
        for (index, sample) in set.set.samples.iter().enumerate() {
            let started = Instant::now();
            let heard = transcriber
                .transcribe(&sample.audio, sample.language)
                .map(|transcript| transcript.text)
                .map_err(|error| {
                    eprintln!("{name}: {error}");
                    TranscriptionFailure::EngineFailed
                });
            let took = started.elapsed();
            let line = serde_json::json!({
                "model": name,
                "set": set.name,
                "sample": index + 1,
                "speaker": set.set.recorded_with.speaker,
                "language": sample.language.tag(),
                "input": input_name(sample.input),
                "kind": sample.kind.name(),
                "conditions": sample.conditions,
                "said": sample.said,
                "prompt": sample.prompt,
                "heard": heard.as_ref().ok(),
                "seconds_of_audio": sample.audio.duration().as_secs_f64(),
                "took_ms": took.as_secs_f64() * 1000.0,
            });
            writeln!(report_file, "{line}").map_err(|error| error.to_string())?;
            scored.push(Scored {
                kind: sample.kind,
                language: sample.language,
                speaker: set.set.recorded_with.speaker.clone(),
                input: sample.input,
                said: sample.said.clone(),
                heard,
                audio: sample.audio.duration(),
                took,
            });
        }
    }
    let name = name.trim_end_matches("-Q8_0");
    print_summary(name, "everything", &summarise(&scored));
    let groups = [
        by(&scored, |sample| {
            format!("language {}", sample.language.tag())
        }),
        by(&scored, |sample| {
            format!("mic {}", input_name(sample.input))
        }),
        by(&scored, |sample| format!("speaker {}", sample.speaker)),
    ];
    for group in groups.iter().filter(|group| group.len() > 1) {
        for (label, summary) in group {
            print_summary(name, label, summary);
        }
    }
    for (kind, summary) in by_kind(&scored) {
        print_summary(name, kind.name(), &summary);
    }
    let used = cost::so_far()?;
    let minutes = scored
        .iter()
        .map(|sample| sample.audio)
        .sum::<Duration>()
        .as_secs_f64()
        / 60.0;
    let on_disk = std::fs::metadata(model).map(|file| file.len()).unwrap_or(0);
    println!(
        "{:<24} {:<36} {:.2} GB on disk, {:.2} GB peak memory, {:.1} CPU s per minute of audio (GPU not counted)",
        name,
        "cost",
        on_disk as f64 / 1e9,
        used.peak_footprint_bytes as f64 / 1e9,
        used.cpu.as_secs_f64() / minutes,
    );
    Ok(())
}

fn model_name(model: &str) -> String {
    Path::new(model)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| model.to_string())
}

const SCORE_ONE_MODEL: &str = "--score-one-model";

fn input_name(input: InputKind) -> &'static str {
    match input {
        InputKind::BuiltIn => "built-in",
        InputKind::Other => "other",
    }
}

fn print_summary(model: &str, label: &str, summary: &Summary) {
    println!(
        "{:<24} {:<36} {:>7} {:>10} {:>9} {:>9} {:>12} {:>10.0}",
        model,
        label,
        summary.samples,
        summary
            .word_error_rate
            .map_or("-".to_string(), |rate| format!("{:.1}%", rate * 100.0)),
        summary.invented_text,
        summary.failures,
        summary
            .times_real_time
            .map_or("-".to_string(), |times| format!("{times:.0}x")),
        summary.median_took.as_secs_f64() * 1000.0
    );
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
