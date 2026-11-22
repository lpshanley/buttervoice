mod common;

#[test]
fn a_panic_message_built_at_run_time_is_left_out_of_the_log() {
    let log_dir = tempfile::tempdir().unwrap();
    let telemetry = buttervoice_telemetry::install(log_dir.path()).unwrap();
    let heard = String::from("my door code is 4471");

    let outcome = std::thread::spawn(move || panic!("could not type `{heard}`")).join();
    drop(telemetry);

    assert!(outcome.is_err());
    let lines = common::read_lines(log_dir.path());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "panic");
    assert_eq!(lines[0].get("panic_message"), None);
    assert!(!lines[0].to_string().contains("4471"));
}
