mod common;

use buttervoice_core::capture::{AudioFormat, CapturedAudio};
use buttervoice_core::language::Language;
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use buttervoice_grok::{FillerWords, GrokTranscriber};
use common::TestServer;

fn a_little_audio() -> CapturedAudio {
    CapturedAudio {
        format: AudioFormat {
            sample_rate: 48_000,
            channels: 1,
        },
        samples: vec![0.25, -0.25],
        dropped_samples: 0,
    }
}

fn transcriber(base_url: &str) -> GrokTranscriber {
    GrokTranscriber::new(base_url, "test-key".to_string()).unwrap()
}

#[test]
fn returns_the_text_the_service_heard() {
    let server = TestServer::responding(
        200,
        r#"{"text":" Hello world. ","language":"en","duration":1.25}"#,
    );

    let transcript = transcriber(&server.base_url).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );

    assert_eq!(
        transcript,
        Ok(Transcript {
            text: "Hello world.".to_string()
        })
    );
}

#[test]
fn uploads_the_audio_as_a_wav_file_with_the_api_key() {
    let server = TestServer::responding(200, r#"{"text":"","language":"en","duration":0.0}"#);

    let _ = transcriber(&server.base_url).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );

    let request = server.received();
    assert_eq!(request.request_line, "POST /stt HTTP/1.1");
    assert!(request.headers.contains("authorization: bearer test-key"));
    let body = request.body_text();
    assert!(body.contains(r#"name="language""#));
    assert!(body.contains("RIFF"), "the upload should be a WAV file");
}

#[test]
fn sends_the_file_last_because_the_service_may_ignore_fields_after_it() {
    let server = TestServer::responding(200, r#"{"text":"","language":"en","duration":0.0}"#);

    let _ = transcriber(&server.base_url).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );

    let body = server.received().body_text();
    let position = |field: &str| {
        body.find(&format!(r#"name="{field}""#))
            .unwrap_or_else(|| panic!("the upload should have a {field} field"))
    };
    assert!(position("file") > position("language"));
    assert!(position("file") > position("format"));
}

#[test]
fn says_why_the_service_refused() {
    for (status, expected) in [
        (401, TranscriptionFailure::Unauthorized),
        (403, TranscriptionFailure::Unauthorized),
        (429, TranscriptionFailure::Unavailable),
        (500, TranscriptionFailure::Unavailable),
        (503, TranscriptionFailure::Unavailable),
        (400, TranscriptionFailure::Rejected),
        (413, TranscriptionFailure::Rejected),
    ] {
        let server = TestServer::responding(status, r#"{"error":"no"}"#);

        let outcome = transcriber(&server.base_url).transcribe(
            &a_little_audio(),
            Language::English,
            FillerWords::Dropped,
        );

        assert_eq!(outcome, Err(expected), "for status {status}");
    }
}

#[test]
fn a_reply_that_is_not_a_transcript_counts_as_the_service_being_unavailable() {
    let server = TestServer::responding(200, "<html>bad gateway</html>");

    let outcome = transcriber(&server.base_url).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );

    assert_eq!(outcome, Err(TranscriptionFailure::Unavailable));
}

#[test]
fn a_service_that_cannot_be_reached_is_reported_as_unreachable() {
    let outcome = transcriber(&common::unused_base_url()).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );

    assert_eq!(outcome, Err(TranscriptionFailure::Unreachable));
}

#[test]
fn asks_for_a_voice_gate_threshold_only_when_one_is_set() {
    let reply = r#"{"text":"","language":"en","duration":0.0}"#;

    let server = TestServer::responding(200, reply);
    let _ = transcriber(&server.base_url).transcribe(
        &a_little_audio(),
        Language::English,
        FillerWords::Dropped,
    );
    assert!(!server.received().body_text().contains("vad_threshold"));

    let server = TestServer::responding(200, reply);
    let _ = transcriber(&server.base_url)
        .with_vad_threshold(0.1)
        .transcribe(&a_little_audio(), Language::English, FillerWords::Dropped);
    let body = server.received().body_text();
    let threshold = body
        .find("name=\"vad_threshold\"\r\n\r\n0.1\r\n")
        .expect("the upload should carry vad_threshold = 0.1");
    assert!(threshold < body.find("name=\"file\"").unwrap());
}

#[test]
fn tells_the_service_which_language_to_expect() {
    for (language, code) in [
        (Language::English, "en"),
        (Language::BrazilianPortuguese, "pt"),
    ] {
        let server = TestServer::responding(200, r#"{"text":"","language":"en","duration":0.0}"#);

        let _ = transcriber(&server.base_url).transcribe(
            &a_little_audio(),
            language,
            FillerWords::Dropped,
        );

        let body = server.received().body_text();
        assert!(
            body.contains(&format!("name=\"language\"\r\n\r\n{code}\r\n")),
            "expected the language field to be {code}"
        );
    }
}

#[test]
fn asks_the_service_to_keep_thinking_sounds_only_when_they_are_to_be_kept() {
    let sent = |filler_words| {
        let server = TestServer::responding(200, r#"{"text":"","language":"en","duration":0.0}"#);
        let _ = transcriber(&server.base_url).transcribe(
            &a_little_audio(),
            Language::English,
            filler_words,
        );
        server.received().body_text()
    };

    assert!(sent(FillerWords::Kept).contains("name=\"filler_words\"\r\n\r\ntrue"));
    assert!(!sent(FillerWords::Dropped).contains("name=\"filler_words\""));
}
