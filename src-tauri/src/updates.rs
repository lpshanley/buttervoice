//! Checking for a newer release and installing it. Only ever when the
//! person asks: a check contacts GitHub.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATE_PROGRESS: &str = "update-progress";

#[derive(serde::Serialize)]
pub struct Found {
    current: String,
    /// `None` when this is the newest release.
    newer: Option<String>,
}

/// The update found by the last check, kept so installing it does not
/// mean asking GitHub again.
#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
}

impl Updates {
    pub async fn check(&self, app: &AppHandle) -> Result<Found, String> {
        let update = app
            .updater()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| error.to_string())?;
        let newer = update.as_ref().map(|update| update.version.clone());
        *self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = update;
        Ok(Found {
            current: app.package_info().version.to_string(),
            newer,
        })
    }

    /// The newer version found by the last check, without asking again.
    pub fn known(&self) -> Option<String> {
        self.found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|update| update.version.clone())
    }

    /// Downloads and installs the update found by the last check, then
    /// relaunches into it.
    pub async fn install(&self, app: &AppHandle) -> Result<(), String> {
        let update = self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or("no update has been found")?;
        let mut progress = DownloadProgress::default();
        update
            .download_and_install(
                |chunk, total| {
                    if let Some(percent) = progress.advance(chunk, total) {
                        let _ = app.emit_to(crate::MAIN_WINDOW, UPDATE_PROGRESS, percent);
                    }
                },
                || {
                    let _ = app.emit_to(crate::MAIN_WINDOW, UPDATE_PROGRESS, 100u8);
                },
            )
            .await
            .map_err(|error| {
                // So the window stops showing a download that ended.
                let _ = app.emit_to(crate::MAIN_WINDOW, UPDATE_PROGRESS, None::<u8>);
                error.to_string()
            })?;
        tracing::info!(
            version = update.version,
            "an update was installed; relaunching"
        );
        app.restart()
    }
}

/// How much of an update has downloaded, as a whole percent, told only when
/// it rises so the window is not sent an event for every chunk.
#[derive(Default)]
pub struct DownloadProgress {
    downloaded: u64,
    told: u8,
}

impl DownloadProgress {
    pub fn advance(&mut self, chunk: usize, total: Option<u64>) -> Option<u8> {
        self.downloaded += chunk as u64;
        let total = total.filter(|&total| total > 0)?;
        let percent = (self.downloaded.min(total) * 100 / total) as u8;
        (percent > self.told).then(|| {
            self.told = percent;
            percent
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_each_whole_percent_once() {
        let mut progress = DownloadProgress::default();
        assert_eq!(progress.advance(5, Some(1000)), None);
        assert_eq!(progress.advance(5, Some(1000)), Some(1));
        assert_eq!(progress.advance(4, Some(1000)), None);
        assert_eq!(progress.advance(616, Some(1000)), Some(63));
        assert_eq!(progress.advance(370, Some(1000)), Some(100));
    }

    #[test]
    fn reports_nothing_when_the_size_is_unknown() {
        let mut progress = DownloadProgress::default();
        assert_eq!(progress.advance(4096, None), None);
    }
}
