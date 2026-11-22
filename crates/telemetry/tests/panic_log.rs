mod common;

use std::sync::mpsc;

#[test]
fn a_panic_on_any_thread_is_logged_with_where_it_happened() {
    let log_dir = tempfile::tempdir().unwrap();
    let telemetry = buttervoice_telemetry::install(log_dir.path()).unwrap();
    let (panic_line, line_of_the_panic) = mpsc::channel();

    let outcome = std::thread::Builder::new()
        .name("microphone".into())
        .spawn(move || {
            panic_line.send(line!() + 1).unwrap();
            panic!("the stream stopped");
        })
        .unwrap()
        .join();
    drop(telemetry);

    assert!(outcome.is_err());
    let lines = common::read_lines(log_dir.path());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["level"], "ERROR");
    assert_eq!(lines[0]["message"], "panic");
    assert_eq!(
        lines[0]["location"],
        format!(
            "crates/telemetry/tests/panic_log.rs:{}",
            line_of_the_panic.recv().unwrap()
        )
    );
    assert_eq!(lines[0]["thread"], "microphone");
    assert_eq!(lines[0]["panic_message"], "the stream stopped");
}
