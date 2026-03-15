use std::process::Command;

use anyhow::{anyhow, Context, Result};

const SERVICE: &str = "ButterVoice.LlmCleanup";
const LLM_ACCOUNT: &str = "llm_api_key";
const SPEECH_ACCOUNT: &str = "speech_api_key";

fn load_api_key(account: &str) -> Result<Option<String>> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a", account, "-w"])
        .output()
        .context("failed reading API key from macOS Keychain")?;

    if output.status.success() {
        let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if key.is_empty() {
            Ok(None)
        } else {
            Ok(Some(key))
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("could not be found") {
            Ok(None)
        } else {
            Err(anyhow!(
                "failed reading API key from Keychain: {}",
                stderr.trim()
            ))
        }
    }
}

fn store_api_key(account: &str, key: &str) -> Result<bool> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        clear_api_key(account)?;
        return Ok(false);
    }

    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            SERVICE,
            "-a",
            account,
            "-w",
            trimmed,
        ])
        .output()
        .context("failed storing API key in macOS Keychain")?;

    if output.status.success() {
        Ok(true)
    } else {
        Err(anyhow!(
            "failed storing API key in Keychain: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn clear_api_key(account: &str) -> Result<()> {
    let output = Command::new("security")
        .args(["delete-generic-password", "-s", SERVICE, "-a", account])
        .output()
        .context("failed clearing API key from macOS Keychain")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stderr.contains("could not be found") {
        Ok(())
    } else {
        Err(anyhow!(
            "failed deleting API key from Keychain: {}",
            stderr.trim()
        ))
    }
}

pub fn load_llm_api_key() -> Result<Option<String>> {
    load_api_key(LLM_ACCOUNT)
}

pub fn store_llm_api_key(key: &str) -> Result<bool> {
    store_api_key(LLM_ACCOUNT, key)
}

pub fn load_speech_api_key() -> Result<Option<String>> {
    load_api_key(SPEECH_ACCOUNT)
}

pub fn store_speech_api_key(key: &str) -> Result<bool> {
    store_api_key(SPEECH_ACCOUNT, key)
}
