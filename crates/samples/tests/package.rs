use std::io::Write;
use std::path::Path;

use buttervoice_core::capture::{AudioFormat, CapturedAudio, InputKind};
use buttervoice_core::language::Language;
use buttervoice_samples::package::{PACKAGE_VERSION, PackError, UnpackError, pack, unpack};
use buttervoice_samples::{Kind, RecordedWith, Sample, SampleSet, load, save};
use zip::write::SimpleFileOptions;

fn a_set() -> SampleSet {
    SampleSet {
        recorded_with: RecordedWith {
            speaker: "speaker-a".to_string(),
            app_version: "0.2.0".to_string(),
            app_commit: "0a1b2c3d4e5f".to_string(),
            chip: "Apple M5 Pro".to_string(),
            macos: "27.2".to_string(),
        },
        samples: vec![Sample {
            kind: Kind::ReadSentence,
            language: Language::BrazilianPortuguese,
            said: "Bom dia.".to_string(),
            prompt: Some("Bom dia.".to_string()),
            conditions: None,
            input: InputKind::Other,
            audio: CapturedAudio {
                format: AudioFormat {
                    sample_rate: 24_000,
                    channels: 1,
                },
                samples: vec![0.25, -0.5, 0.125],
                dropped_samples: 0,
            },
        }],
    }
}

/// A set saved in a folder called `name`, packed into `package.zip`.
fn a_package(at: &Path, name: &str) -> std::path::PathBuf {
    let set_folder = at.join(name);
    save(&set_folder, &a_set()).unwrap();
    let package = at.join("package.zip");
    pack(&set_folder, &package).unwrap();
    package
}

/// A zip written by hand, entry by entry, as a hostile package could be.
fn a_zip(at: &Path, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let path = at.join("crafted.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn manifest(set: &str) -> Vec<u8> {
    format!(r#"{{"package_version":{PACKAGE_VERSION},"set":"{set}"}}"#).into_bytes()
}

#[test]
fn a_packed_set_unpacks_into_the_suite_exactly_as_it_was() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let package = a_package(sender.path(), "speaker-a-1700000000");

    let unpacked = unpack(&package, suite.path()).unwrap();

    assert_eq!(unpacked, suite.path().join("speaker-a-1700000000"));
    assert_eq!(load(&unpacked), Ok(a_set()));
}

#[test]
fn a_folder_that_is_not_a_set_is_not_packed() {
    let folder = tempfile::tempdir().unwrap();

    let outcome = pack(folder.path(), &folder.path().join("package.zip"));

    assert_eq!(outcome, Err(PackError::NotASet));
    assert!(!folder.path().join("package.zip").exists());
}

#[test]
fn a_set_already_in_the_suite_is_not_unpacked_over() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let package = a_package(sender.path(), "speaker-a-1700000000");
    unpack(&package, suite.path()).unwrap();

    let again = unpack(&package, suite.path());

    assert_eq!(
        again,
        Err(UnpackError::AlreadyInSuite(
            "speaker-a-1700000000".to_string()
        ))
    );
    assert_eq!(
        load(&suite.path().join("speaker-a-1700000000")),
        Ok(a_set())
    );
}

#[test]
fn a_package_that_names_a_file_outside_the_suite_writes_nothing() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    for hostile in [
        "set/../../escaped.txt",
        "../escaped.txt",
        "/tmp/escaped.txt",
        "set/sub/0001.wav",
    ] {
        let package = a_zip(
            sender.path(),
            &[("manifest.json", &manifest("speaker-a")), (hostile, b"x")],
        );

        let outcome = unpack(&package, suite.path());

        assert_eq!(
            outcome,
            Err(UnpackError::UnsafeName(hostile.to_string())),
            "{hostile}"
        );
        assert!(
            std::fs::read_dir(suite.path()).unwrap().next().is_none(),
            "{hostile}"
        );
    }
}

#[test]
fn a_set_name_that_would_leave_the_suite_is_refused() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let package = a_zip(
        sender.path(),
        &[("manifest.json", &manifest("../elsewhere"))],
    );

    let outcome = unpack(&package, suite.path());

    assert_eq!(
        outcome,
        Err(UnpackError::UnsafeName("../elsewhere".to_string()))
    );
}

#[test]
fn a_package_from_a_version_this_one_cannot_read_is_refused_by_its_version() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let package = a_zip(
        sender.path(),
        &[(
            "manifest.json",
            br#"{"package_version":99,"set":"speaker-a"}"#,
        )],
    );

    assert_eq!(
        unpack(&package, suite.path()),
        Err(UnpackError::UnknownVersion(99))
    );
}

#[test]
fn a_file_that_is_not_a_package_is_refused() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let not_a_zip = sender.path().join("notes.zip");
    std::fs::write(&not_a_zip, "just some text").unwrap();
    let no_manifest = a_zip(sender.path(), &[("set/set.json", b"{}")]);

    assert_eq!(
        unpack(&not_a_zip, suite.path()),
        Err(UnpackError::NotAPackage)
    );
    assert_eq!(
        unpack(&no_manifest, suite.path()),
        Err(UnpackError::NotAPackage)
    );
}

#[test]
fn a_package_whose_set_cannot_be_read_leaves_nothing_in_the_suite() {
    let sender = tempfile::tempdir().unwrap();
    let suite = tempfile::tempdir().unwrap();
    let package = a_zip(
        sender.path(),
        &[
            ("manifest.json", &manifest("speaker-a")),
            ("set/set.json", b"not json"),
        ],
    );

    let outcome = unpack(&package, suite.path());

    assert!(
        matches!(outcome, Err(UnpackError::BrokenSet(_))),
        "{outcome:?}"
    );
    assert!(std::fs::read_dir(suite.path()).unwrap().next().is_none());
}
