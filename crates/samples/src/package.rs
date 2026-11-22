//! A sample set as one file to send, and taking one into a suite.
//!
//! A package is a zip holding `manifest.json` and the set's files under
//! `set/`. Unpacking trusts nothing in it: it came from someone else.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Raised whenever a package written by this version could not be read by
/// the one before it.
pub const PACKAGE_VERSION: u32 = 1;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum PackError {
    #[error("this folder is not a sample set")]
    NotASet,
    #[error("the package could not be written: {0}")]
    Write(String),
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum UnpackError {
    #[error("this file is not a samples package")]
    NotAPackage,
    #[error("the package was made in version {0}, which this version cannot read")]
    UnknownVersion(u32),
    #[error("the package holds a file named {0:?}, which is not allowed")]
    UnsafeName(String),
    #[error("a set called {0:?} is already in the suite")]
    AlreadyInSuite(String),
    #[error("the set in the package could not be read: {0}")]
    BrokenSet(String),
    #[error("the set could not be written: {0}")]
    Write(String),
}

const MANIFEST: &str = "manifest.json";
const SET_PREFIX: &str = "set/";

#[derive(Serialize, Deserialize)]
struct Manifest {
    package_version: u32,
    set: String,
}

/// Only the set's own files go in: its description and the audio it names,
/// never anything else that happens to sit in the folder.
pub fn pack(set_folder: &Path, package: &Path) -> Result<(), PackError> {
    crate::load(set_folder).map_err(|_| PackError::NotASet)?;
    let write = |error: &dyn std::fmt::Display| PackError::Write(error.to_string());
    let description =
        std::fs::read_to_string(set_folder.join(crate::DESCRIPTION)).map_err(|e| write(&e))?;
    let audio_files: Vec<String> = serde_json::from_str::<serde_json::Value>(&description)
        .map_err(|e| write(&e))?["samples"]
        .as_array()
        .map(|samples| {
            samples
                .iter()
                .filter_map(|sample| sample["audio"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let name = set_folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let manifest = serde_json::to_string(&Manifest {
        package_version: PACKAGE_VERSION,
        set: name,
    })
    .map_err(|e| write(&e))?;

    let mut zip = ZipWriter::new(std::fs::File::create(package).map_err(|e| write(&e))?);
    let options = SimpleFileOptions::default();
    zip.start_file(MANIFEST, options).map_err(|e| write(&e))?;
    zip.write_all(manifest.as_bytes()).map_err(|e| write(&e))?;
    for file in std::iter::once(crate::DESCRIPTION.to_string()).chain(audio_files) {
        let bytes = std::fs::read(set_folder.join(&file)).map_err(|e| write(&e))?;
        zip.start_file(format!("{SET_PREFIX}{file}"), options)
            .map_err(|e| write(&e))?;
        zip.write_all(&bytes).map_err(|e| write(&e))?;
    }
    zip.finish().map_err(|e| write(&e))?;
    Ok(())
}

/// Every name is checked before anything is written, and the set is only
/// given its name in the suite once it has been read back whole.
pub fn unpack(package: &Path, suite: &Path) -> Result<PathBuf, UnpackError> {
    let file = std::fs::File::open(package).map_err(|_| UnpackError::NotAPackage)?;
    let mut zip = ZipArchive::new(file).map_err(|_| UnpackError::NotAPackage)?;
    let manifest: Manifest = {
        let mut entry = zip
            .by_name(MANIFEST)
            .map_err(|_| UnpackError::NotAPackage)?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|_| UnpackError::NotAPackage)?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| UnpackError::NotAPackage)?;
        let version = value["package_version"]
            .as_u64()
            .ok_or(UnpackError::NotAPackage)?;
        if version != u64::from(PACKAGE_VERSION) {
            return Err(UnpackError::UnknownVersion(version as u32));
        }
        serde_json::from_value(value).map_err(|_| UnpackError::NotAPackage)?
    };
    if !plain_name(&manifest.set) {
        return Err(UnpackError::UnsafeName(manifest.set));
    }
    let destination = suite.join(&manifest.set);
    if destination.exists() {
        return Err(UnpackError::AlreadyInSuite(manifest.set));
    }

    let mut files = Vec::new();
    for index in 0..zip.len() {
        let name = zip
            .by_index(index)
            .map_err(|_| UnpackError::NotAPackage)?
            .name()
            .map_err(|_| UnpackError::NotAPackage)?
            .into_owned();
        if name == MANIFEST {
            continue;
        }
        match name.strip_prefix(SET_PREFIX) {
            Some(file) if plain_name(file) => files.push((index, file.to_string())),
            _ => return Err(UnpackError::UnsafeName(name)),
        }
    }

    let unpacking = suite.join(format!(".unpacking-{}", manifest.set));
    let written = (|| -> Result<(), UnpackError> {
        let write = |error: std::io::Error| UnpackError::Write(error.to_string());
        std::fs::create_dir_all(&unpacking).map_err(write)?;
        for (index, file) in &files {
            let mut entry = zip.by_index(*index).map_err(|_| UnpackError::NotAPackage)?;
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|_| UnpackError::NotAPackage)?;
            std::fs::write(unpacking.join(file), bytes).map_err(write)?;
        }
        crate::load(&unpacking).map_err(|error| UnpackError::BrokenSet(error.to_string()))?;
        std::fs::rename(&unpacking, &destination).map_err(write)
    })();
    if written.is_err() {
        let _ = std::fs::remove_dir_all(&unpacking);
    }
    written.map(|()| destination)
}

/// A single file or folder name of letters, digits, `.`, `-` and `_`, not
/// starting with a dot, so it cannot climb out of where it is put.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}
