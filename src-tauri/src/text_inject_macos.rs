use anyhow::{Context, Result};
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

pub fn inject_text(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    // Primary path: simulate text typing directly into focused application.
    let mut enigo =
        Enigo::new(&Settings::default()).context("failed creating enigo keyboard driver")?;
    enigo.text(text).context("failed typing text via enigo")?;

    Ok(())
}

pub fn inject_with_clipboard_fallback(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    let mut clipboard = Clipboard::new().context("failed creating clipboard handle")?;
    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .context("failed setting clipboard text")?;

    let mut enigo =
        Enigo::new(&Settings::default()).context("failed creating enigo keyboard driver")?;
    enigo
        .key(Key::Meta, Direction::Press)
        .context("failed pressing Meta key")?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .context("failed clicking V key")?;
    enigo
        .key(Key::Meta, Direction::Release)
        .context("failed releasing Meta key")?;

    // Allow time for the target application to read the clipboard before
    // restoring previous contents.  The Cmd+V key event is delivered
    // asynchronously — the receiving app must service its event loop and
    // perform the paste read.  Without this delay the clipboard can be
    // overwritten before the paste completes, inserting stale content.
    std::thread::sleep(std::time::Duration::from_millis(75));

    if let Some(previous) = previous {
        let _ = clipboard.set_text(previous);
    }

    Ok(())
}
