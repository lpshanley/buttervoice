//! Checking for a newer release and installing it. Only ever when the
//! person asks: a check contacts GitHub.

use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

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

    /// Downloads and installs the update found by the last check, then
    /// relaunches into it.
    pub async fn install(&self, app: &AppHandle) -> Result<(), String> {
        let update = self
            .found
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or("no update has been found")?;
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|error| error.to_string())?;
        tracing::info!(
            version = update.version,
            "an update was installed; relaunching"
        );
        app.restart()
    }
}
