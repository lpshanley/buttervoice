//! The choices that outlive a run, kept as JSON in the app's own folder.

use std::path::Path;

use buttervoice_core::transcription::Provider;
use serde_json::{Value, json};

const PROVIDER: &str = "provider";

pub fn load_provider(file: &Path) -> Provider {
    let saved = std::fs::read_to_string(file)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok());
    let name = saved
        .as_ref()
        .and_then(|preferences| preferences.get(PROVIDER))
        .and_then(Value::as_str);
    Provider::from_setting(name)
}

pub fn save_provider(file: &Path, provider: Provider) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, json!({ PROVIDER: provider.name() }).to_string())
}
