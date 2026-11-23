//! Checking for a newer release and installing it. Only ever when the
//! person asks: a check contacts GitHub.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Runtime};
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
    pub async fn check<R: Runtime>(&self, app: &AppHandle<R>) -> Result<Found, String> {
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
    pub async fn install<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String> {
        let update = self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            // Cloned, not taken, so "Try again" still has it after a failure.
            .clone()
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
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

    use super::*;

    /// Stands in for GitHub: a feed naming a newer release, whose download
    /// always fails. Returns the feed's address.
    fn release_with_failing_download() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let feed = serde_json::json!({
            "version": "99.0.0",
            "platforms": {
                "test-target": { "url": format!("{server}/download"), "signature": "unused" }
            }
        })
        .to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut request = [0; 4096];
                let read = stream.read(&mut request).unwrap_or(0);
                let response = if request[..read].starts_with(b"GET /feed ") {
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                         content-length: {}\r\nconnection: close\r\n\r\n{feed}",
                        feed.len()
                    )
                } else {
                    "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\
                     connection: close\r\n\r\n"
                        .to_string()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("{server}/feed")
    }

    fn app_checking(feed: &str) -> tauri::App<MockRuntime> {
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
                // The download fails before any signature is checked.
                "pubkey": "unused",
                "endpoints": [feed],
                "dangerousInsecureTransportProtocol": true,
            }),
        );
        mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .target("test-target")
                    .build(),
            )
            .build(context)
            .unwrap()
    }

    #[test]
    fn a_failed_install_can_be_tried_again() {
        let app = app_checking(&release_with_failing_download());
        let updates = Updates::default();
        tauri::async_runtime::block_on(async {
            updates.check(app.handle()).await.unwrap();
            let failed = updates.install(app.handle()).await.unwrap_err();

            assert_eq!(updates.known().as_deref(), Some("99.0.0"));
            let retried = updates.install(app.handle()).await.unwrap_err();
            assert_eq!(
                retried, failed,
                "trying again should retry the same download"
            );
        });
    }

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
