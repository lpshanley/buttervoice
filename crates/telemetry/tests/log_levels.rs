mod common;

#[test]
fn chatter_below_info_level_is_kept_out_of_the_log() {
    let log_dir = tempfile::tempdir().unwrap();
    let (subscriber, guard) = buttervoice_telemetry::file_subscriber(log_dir.path()).unwrap();
    tracing::subscriber::with_default(subscriber, || {
        tracing::trace!("connection pool detail");
        tracing::debug!("connecting to a server");
        tracing::info!("worth keeping");
    });
    drop(guard);

    let messages: Vec<String> = common::read_lines(log_dir.path())
        .iter()
        .map(|line| line["message"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(messages, ["worth keeping"]);
}
