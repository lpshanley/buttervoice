use std::process::Command;

fn main() {
    println!("cargo:rustc-env=BUTTERVOICE_COMMIT={}", commit());
    tauri_build::build()
}

fn commit() -> String {
    let Some(commit) = git(&["rev-parse", "--short=12", "HEAD"]) else {
        return "unknown".into();
    };
    if std::env::var("PROFILE").is_ok_and(|profile| profile == "release") {
        // Cargo re-runs this script on every build when the path is missing.
        // Only that keeps the mark below true, and it would cost every
        // development build a recompile, so only release builds carry it.
        println!("cargo:rerun-if-changed=always-rerun-in-release");
        let tracked_changes = git(&["status", "--porcelain", "--untracked-files=no"]);
        if tracked_changes.is_none_or(|changes| !changes.is_empty()) {
            return format!("{commit}-dirty");
        }
    } else {
        // The log of HEAD grows on every commit, checkout and reset.
        for file in ["HEAD", "logs/HEAD"] {
            if let Some(path) = git(&["rev-parse", "--git-path", file]) {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    commit
}

fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
