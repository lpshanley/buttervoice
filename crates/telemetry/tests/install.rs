mod common;

#[test]
fn after_install_an_event_from_any_thread_lands_in_the_log_directory() {
    let log_dir = tempfile::tempdir().unwrap();
    let guard = buttervoice_telemetry::install(log_dir.path()).unwrap();

    std::thread::spawn(|| tracing::info!(answer = 42, "from another thread"))
        .join()
        .unwrap();
    drop(guard);

    let lines = common::read_lines(log_dir.path());
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["message"], "from another thread");
    assert_eq!(lines[0]["answer"], 42);
}
