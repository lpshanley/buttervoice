mod common;

use buttervoice_telemetry::{Build, BuildKind, file_subscriber, record_app_started};
use serde_json::Value;

fn lines_logged(build: &Build) -> Vec<Value> {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || record_app_started(build));
    drop(guard);
    common::read_lines(log_dir.path())
}

#[test]
fn the_app_started_line_says_which_build_wrote_it() {
    let lines = lines_logged(&Build {
        version: "1.2.3",
        kind: BuildKind::Release,
        commit: "0a1b2c3d4e5f",
    });

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "app started");
    assert_eq!(lines[0]["version"], "1.2.3");
    assert_eq!(lines[0]["build"], "release");
    assert_eq!(lines[0]["commit"], "0a1b2c3d4e5f");
}

#[test]
fn a_debug_build_is_named_as_one() {
    let lines = lines_logged(&Build {
        version: "1.2.3",
        kind: BuildKind::Debug,
        commit: "0a1b2c3d4e5f",
    });

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["build"], "debug");
}
