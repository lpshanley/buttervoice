use buttervoice_core::capture::{AudioFormat, CapturedAudio};
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use buttervoice_replay::recordings::Recording;
use buttervoice_replay::report::replay;

/// A recording whose single sample says which one it is.
fn recording(name: &str, marker: f32) -> Recording {
    Recording {
        name: name.to_string(),
        audio: CapturedAudio {
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 1,
            },
            samples: vec![marker],
            dropped_samples: 0,
        },
    }
}

fn heard(text: &str) -> Result<Transcript, TranscriptionFailure> {
    Ok(Transcript {
        text: text.to_string(),
    })
}

#[test]
fn each_recording_is_scored_against_the_sentence_read_for_it() {
    let recordings = [recording("001", 1.0), recording("002", 2.0)];
    let references = [
        "turn the lights off".to_string(),
        "good morning".to_string(),
    ];

    let report = replay(&recordings, &references, |audio| {
        if audio.samples[0] == 1.0 {
            heard("turn the light off")
        } else {
            heard("good morning")
        }
    });

    let scores: Vec<(String, f64)> = report
        .heard
        .iter()
        .map(|heard| (heard.recording.clone(), heard.word_error_rate))
        .collect();
    assert_eq!(
        scores,
        [("001".to_string(), 0.25), ("002".to_string(), 0.0)]
    );
    assert_eq!(report.mean_word_error_rate(), 0.125);
}

#[test]
fn blank_answers_and_failed_requests_are_counted_separately() {
    let recordings = [
        recording("001", 1.0),
        recording("002", 2.0),
        recording("003", 3.0),
    ];
    let references = vec!["good morning".to_string(); 3];

    let report = replay(&recordings, &references, |audio| match audio.samples[0] {
        1.0 => heard("good morning"),
        2.0 => heard(""),
        _ => Err(TranscriptionFailure::Unavailable),
    });

    assert_eq!(report.blanks(), 1);
    assert_eq!(report.failures(), 1);
    assert_eq!(report.mean_word_error_rate(), 2.0 / 3.0);
}
