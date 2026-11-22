//! Fetching a model file: checked against its published SHA-256 before it
//! is put in place, resumable after an interruption, and cancellable.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::RANGE;
use sha2::{Digest, Sha256};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FetchError {
    #[error("the download could not be reached")]
    Unreachable,
    #[error("the server refused the download with status {0}")]
    Refused(u16),
    #[error("the download stopped part-way; it will resume from there")]
    Interrupted,
    #[error("the downloaded file does not match its published checksum")]
    ChecksumMismatch,
    #[error("the download was cancelled")]
    Cancelled,
    #[error("the file could not be written: {0}")]
    Disk(String),
}

/// What a download must turn out to be.
#[derive(Debug, Clone, Copy)]
pub struct Expected<'a> {
    pub sha256: &'a str,
    pub bytes: u64,
}

/// Fetches `source` to `destination`. Bytes arrive in `destination` with
/// `.partial` added until the whole file is checked, so a half-fetched file
/// is never mistaken for a model. `on_progress` is told how many bytes are
/// on disk so far.
pub fn fetch(
    source: &str,
    expected: Expected<'_>,
    destination: &Path,
    mut on_progress: impl FnMut(u64),
    cancelled: &AtomicBool,
) -> Result<(), FetchError> {
    let disk = |error: std::io::Error| FetchError::Disk(error.to_string());
    if let Some(folder) = destination.parent() {
        std::fs::create_dir_all(folder).map_err(disk)?;
    }
    let partial = partial_path(destination);
    let already = std::fs::metadata(&partial).map_or(0, |file| file.len());

    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        // A model takes minutes to arrive: no limit on the whole transfer.
        .timeout(None)
        .build()
        .map_err(|_| FetchError::Unreachable)?;
    let mut request = client.get(source);
    if already > 0 {
        request = request.header(RANGE, format!("bytes={already}-"));
    }
    let mut response = request.send().map_err(|_| FetchError::Unreachable)?;
    let status = response.status();
    // A server that ignores the range sends the whole file again.
    let resuming = status == StatusCode::PARTIAL_CONTENT;
    if !status.is_success() {
        return Err(FetchError::Refused(status.as_u16()));
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&partial)
        .map_err(disk)?;
    let mut on_disk = if resuming { already } else { 0 };

    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(FetchError::Cancelled);
        }
        let read = response
            .read(&mut buffer)
            .map_err(|_| FetchError::Interrupted)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(disk)?;
        on_disk += read as u64;
        on_progress(on_disk);
    }
    file.sync_all().map_err(disk)?;
    drop(file);

    if on_disk < expected.bytes {
        return Err(FetchError::Interrupted);
    }
    if on_disk > expected.bytes || sha256_of(&partial).map_err(disk)? != expected.sha256 {
        let _ = std::fs::remove_file(&partial);
        return Err(FetchError::ChecksumMismatch);
    }
    std::fs::rename(&partial, destination).map_err(disk)
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK: usize = 64 * 1024;

fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
