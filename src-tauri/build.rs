#[cfg(target_os = "macos")]
fn clear_quarantine() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let whisper_dir = manifest_dir.join("resources/whispercpp");

    if !whisper_dir.exists() {
        return;
    }

    let _ = Command::new("xattr")
        .arg("-dr")
        .arg("com.apple.quarantine")
        .arg(&whisper_dir)
        .status();
}

fn main() {
    #[cfg(target_os = "macos")]
    clear_quarantine();

    tauri_build::build()
}
