mod common;

use buttervoice_core::DictationId;
use buttervoice_telemetry::{file_subscriber, record_marked_bad};
use serde_json::Value;

fn lines_logged(latest_dictation: Option<DictationId>) -> Vec<Value> {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || record_marked_bad(latest_dictation));
    drop(guard);
    common::read_lines(log_dir.path())
}

#[test]
fn marking_a_dictation_as_bad_logs_one_line_naming_it() {
    let lines = lines_logged(Some(DictationId(7)));

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "dictation marked bad");
    assert_eq!(lines[0]["dictation_id"], 7);
}

#[test]
fn a_mark_before_any_dictation_is_still_logged_without_an_id() {
    let lines = lines_logged(None);

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "dictation marked bad");
    assert_eq!(lines[0].get("dictation_id"), None);
}
