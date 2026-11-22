mod common;

use buttervoice_core::capture::{AudioFormat, CapturedAudio};
use buttervoice_core::transcription::{Transcript, TranscriptionFailure};
use buttervoice_grok::GrokTranscriber;
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

    let transcript = transcriber(&server.base_url).transcribe(&a_little_audio());

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

    let _ = transcriber(&server.base_url).transcribe(&a_little_audio());

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

    let _ = transcriber(&server.base_url).transcribe(&a_little_audio());

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

        let outcome = transcriber(&server.base_url).transcribe(&a_little_audio());

        assert_eq!(outcome, Err(expected), "for status {status}");
    }
}

#[test]
fn a_reply_that_is_not_a_transcript_counts_as_the_service_being_unavailable() {
    let server = TestServer::responding(200, "<html>bad gateway</html>");

    let outcome = transcriber(&server.base_url).transcribe(&a_little_audio());

    assert_eq!(outcome, Err(TranscriptionFailure::Unavailable));
}

#[test]
fn a_service_that_cannot_be_reached_is_reported_as_unreachable() {
    let outcome = transcriber(&common::unused_base_url()).transcribe(&a_little_audio());

    assert_eq!(outcome, Err(TranscriptionFailure::Unreachable));
}
