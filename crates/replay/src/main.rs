//! Runs saved recordings through transcription in several variants and
//! scores each against the sentence that was read.
//!
//!     XAI_API_KEY=... cargo run -p buttervoice-replay -- replay/recordings replay/sentences.txt

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use buttervoice_grok::{DEFAULT_BASE_URL, GrokTranscriber};
use buttervoice_replay::recordings;
use buttervoice_replay::report::{Report, replay};

struct Variant {
    name: &'static str,
    /// Boosted the way the app boosts audio before transcription.
    boosted: bool,
    vad_threshold: Option<f32>,
}

const VARIANTS: [Variant; 4] = [
    Variant {
        name: "as captured",
        boosted: false,
        vad_threshold: None,
    },
    Variant {
        name: "boosted",
        boosted: true,
        vad_threshold: None,
    },
    Variant {
        name: "gate 0.1",
        boosted: false,
        vad_threshold: Some(0.1),
    },
    Variant {
        name: "boosted, gate 0.1",
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

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(recordings_dir), Some(sentences_file)) = (args.next(), args.next()) else {
        return Err("usage: buttervoice-replay <recordings folder> <sentences file>".into());
    };
    let api_key = std::env::var("XAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or("XAI_API_KEY is not set")?;

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
    let mut report_file = std::fs::File::create(&report_path)
        .map_err(|error| format!("could not create {}: {error}", report_path.display()))?;

    println!(
        "{:<20} {:>10} {:>7} {:>9}",
        "variant", "word errs", "blanks", "failures"
    );
    for variant in &VARIANTS {
        let mut transcriber = GrokTranscriber::new(DEFAULT_BASE_URL, api_key.clone())
            .map_err(|error| error.to_string())?;
        if let Some(threshold) = variant.vad_threshold {
            transcriber = transcriber.with_vad_threshold(threshold);
        }
        let report = replay(&recordings, references, |audio| {
            if variant.boosted {
                transcriber.transcribe(&audio.boosted_for_transcription())
            } else {
                transcriber.transcribe(audio)
            }
        });
        println!(
            "{:<20} {:>9.1}% {:>7} {:>9}",
            variant.name,
            report.mean_word_error_rate() * 100.0,
            report.blanks(),
            report.failures()
        );
        write_lines(&mut report_file, variant.name, &report)
            .map_err(|error| format!("could not write {}: {error}", report_path.display()))?;
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
