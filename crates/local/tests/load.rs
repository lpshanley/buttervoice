use buttervoice_local::{LoadError, LocalTranscriber};

#[test]
fn a_model_file_that_is_not_there_is_reported_as_not_found() {
    let folder = tempfile::tempdir().unwrap();

    let outcome = LocalTranscriber::load(&folder.path().join("missing.gguf"));

    assert_eq!(outcome.err(), Some(LoadError::NotFound));
}
