mod common;

use std::sync::atomic::{AtomicBool, Ordering};

use buttervoice_local::fetch::{Expected, FetchError, fetch};
use common::{FileServer, Serve, ok};
use sha2::{Digest, Sha256};

/// A megabyte, so a download takes many reads.
fn a_model() -> Vec<u8> {
    (0..1_048_576u32).map(|n| (n % 251) as u8).collect()
}

fn checksum(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn fetch_into(
    server: &FileServer,
    folder: &std::path::Path,
    sha256: &str,
    cancelled: &AtomicBool,
    on_progress: impl FnMut(u64),
) -> Result<(), FetchError> {
    let expected = Expected {
        sha256,
        bytes: a_model().len() as u64,
    };
    fetch(
        &server.url,
        expected,
        &folder.join("model.gguf"),
        on_progress,
        cancelled,
    )
}

#[test]
fn a_model_is_fetched_checked_and_put_in_place() {
    let folder = tempfile::tempdir().unwrap();
    let server = FileServer::start(a_model(), vec![ok()]);

    let outcome = fetch_into(
        &server,
        folder.path(),
        &checksum(&a_model()),
        &AtomicBool::new(false),
        |_| {},
    );

    assert_eq!(outcome, Ok(()));
    assert_eq!(
        std::fs::read(folder.path().join("model.gguf")).unwrap(),
        a_model()
    );
    assert!(!folder.path().join("model.gguf.partial").exists());
}

#[test]
fn a_file_that_does_not_match_its_checksum_is_not_kept() {
    let folder = tempfile::tempdir().unwrap();
    let server = FileServer::start(a_model(), vec![ok()]);

    let outcome = fetch_into(
        &server,
        folder.path(),
        &checksum(b"something else"),
        &AtomicBool::new(false),
        |_| {},
    );

    assert_eq!(outcome, Err(FetchError::ChecksumMismatch));
    assert!(!folder.path().join("model.gguf").exists());
    assert!(!folder.path().join("model.gguf.partial").exists());
}

#[test]
fn an_interrupted_download_resumes_where_it_stopped() {
    let folder = tempfile::tempdir().unwrap();
    let cut = Serve {
        cut_after: Some(400_000),
        ..ok()
    };
    let server = FileServer::start(a_model(), vec![cut, ok()]);
    let sha = checksum(&a_model());

    let first = fetch_into(
        &server,
        folder.path(),
        &sha,
        &AtomicBool::new(false),
        |_| {},
    );
    let _ = server.request();
    let kept = std::fs::metadata(folder.path().join("model.gguf.partial"))
        .unwrap()
        .len();
    let second = fetch_into(
        &server,
        folder.path(),
        &sha,
        &AtomicBool::new(false),
        |_| {},
    );

    assert_eq!(first, Err(FetchError::Interrupted));
    assert_eq!(kept, 400_000);
    assert!(
        server
            .request()
            .to_lowercase()
            .contains("range: bytes=400000-")
    );
    assert_eq!(second, Ok(()));
    assert_eq!(
        std::fs::read(folder.path().join("model.gguf")).unwrap(),
        a_model()
    );
}

#[test]
fn a_server_that_ignores_the_resume_request_is_fetched_from_the_start() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("model.gguf.partial"), vec![0u8; 400_000]).unwrap();
    let server = FileServer::start(
        a_model(),
        vec![Serve {
            honour_ranges: false,
            ..ok()
        }],
    );

    let outcome = fetch_into(
        &server,
        folder.path(),
        &checksum(&a_model()),
        &AtomicBool::new(false),
        |_| {},
    );

    assert_eq!(outcome, Ok(()));
    assert_eq!(
        std::fs::read(folder.path().join("model.gguf")).unwrap(),
        a_model()
    );
}

#[test]
fn progress_counts_up_to_the_whole_file() {
    let folder = tempfile::tempdir().unwrap();
    let server = FileServer::start(a_model(), vec![ok()]);
    let mut seen = Vec::new();

    fetch_into(
        &server,
        folder.path(),
        &checksum(&a_model()),
        &AtomicBool::new(false),
        |bytes| seen.push(bytes),
    )
    .unwrap();

    assert!(seen.len() > 1, "progress was reported {} times", seen.len());
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(seen.last().copied(), Some(a_model().len() as u64));
}

#[test]
fn cancelling_stops_the_download_and_keeps_what_was_fetched() {
    let folder = tempfile::tempdir().unwrap();
    let server = FileServer::start(a_model(), vec![ok()]);
    let cancelled = AtomicBool::new(false);

    let outcome = fetch_into(
        &server,
        folder.path(),
        &checksum(&a_model()),
        &cancelled,
        |_| cancelled.store(true, Ordering::Relaxed),
    );

    assert_eq!(outcome, Err(FetchError::Cancelled));
    assert!(!folder.path().join("model.gguf").exists());
    let kept = std::fs::metadata(folder.path().join("model.gguf.partial"))
        .unwrap()
        .len();
    assert!(
        kept > 0 && kept < a_model().len() as u64,
        "kept {kept} bytes"
    );
}

#[test]
fn a_server_that_refuses_is_reported_with_its_status() {
    let folder = tempfile::tempdir().unwrap();
    let server = FileServer::start(
        a_model(),
        vec![Serve {
            status: 404,
            ..ok()
        }],
    );

    let outcome = fetch_into(
        &server,
        folder.path(),
        &checksum(&a_model()),
        &AtomicBool::new(false),
        |_| {},
    );

    assert_eq!(outcome, Err(FetchError::Refused(404)));
}

#[test]
fn a_server_that_cannot_be_reached_is_reported_as_such() {
    let folder = tempfile::tempdir().unwrap();
    let nobody = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.gguf", nobody.local_addr().unwrap());
    drop(nobody);
    let expected = Expected {
        sha256: "00",
        bytes: 1,
    };

    let outcome = fetch(
        &url,
        expected,
        &folder.path().join("model.gguf"),
        |_| {},
        &AtomicBool::new(false),
    );

    assert_eq!(outcome, Err(FetchError::Unreachable));
}
