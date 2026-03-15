use anyhow::Result;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStat {
    pub date: String,
    pub word_count: u32,
    pub dictation_count: u32,
    pub recording_seconds: f64,
}

const STORE_FILE: &str = "usage_stats.json";
const STORE_KEY: &str = "daily_stats";
const RETENTION_DAYS: i64 = 90;

pub struct UsageStatsStore {
    app_handle: AppHandle,
}

impl UsageStatsStore {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    /// Record a completed dictation into today's aggregated stats.
    pub fn record_dictation(&self, word_count: u32, recording_duration_ms: u64) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let recording_seconds = recording_duration_ms as f64 / 1000.0;

        if let Err(err) = self.upsert(today, word_count, recording_seconds) {
            eprintln!("failed recording usage stat: {err:#}");
        }
    }

    /// Return all stored daily stats.
    pub fn get_stats(&self) -> Vec<DailyStat> {
        self.load().unwrap_or_default()
    }

    /// Clear all stored usage stats.
    pub fn clear_stats(&self) {
        if let Err(err) = self.save(&[]) {
            eprintln!("failed clearing usage stats: {err:#}");
        }
    }

    fn upsert(&self, date: String, word_count: u32, recording_seconds: f64) -> Result<()> {
        let mut stats = self.load().unwrap_or_default();

        if let Some(entry) = stats.iter_mut().find(|s| s.date == date) {
            entry.word_count += word_count;
            entry.dictation_count += 1;
            entry.recording_seconds += recording_seconds;
        } else {
            stats.push(DailyStat {
                date,
                word_count,
                dictation_count: 1,
                recording_seconds,
            });
        }

        // Prune entries older than retention period
        let cutoff = Local::now()
            .date_naive()
            .checked_sub_days(chrono::Days::new(RETENTION_DAYS as u64));
        if let Some(cutoff_date) = cutoff {
            let cutoff_str = cutoff_date.format("%Y-%m-%d").to_string();
            stats.retain(|s| s.date >= cutoff_str);
        }

        self.save(&stats)
    }

    fn load(&self) -> Result<Vec<DailyStat>> {
        let store = self
            .app_handle
            .store(STORE_FILE)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let stats = store
            .get(STORE_KEY)
            .and_then(|v| serde_json::from_value::<Vec<DailyStat>>(v).ok())
            .unwrap_or_default();

        Ok(stats)
    }

    fn save(&self, stats: &[DailyStat]) -> Result<()> {
        let store = self
            .app_handle
            .store(STORE_FILE)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        store.set(
            STORE_KEY,
            serde_json::to_value(stats)
                .map_err(|e| anyhow::anyhow!("failed serializing usage stats: {e}"))?,
        );
        let _ = store.save();
        Ok(())
    }
}
