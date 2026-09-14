# Recording reliability checks

Automated regression coverage lives in `hotkey_macos`, `recording_control`, and
`audio`. Run the Rust suite and frontend build before testing a new app build.

## Live macOS verification

Use a test text field as the output destination. Verify the ButterVoice HUD and
the macOS microphone indicator separately. With persistent capture enabled, the
microphone indicator is expected to remain on between recordings.

- With Right Option and push-to-talk selected, hold Right Option, hold Left
  Option, release Right Option, then release Left Option. Recording must end
  on the Right Option release. Repeat with the opposite order and the supported
  left/right Control and Command shortcuts.
- Exercise rapid taps and ordinary long holds. A physical press/release pair
  must produce at most one recording and one finalization.
- Test Fn and a custom non-modifier shortcut. Holding a repeating key in toggle
  mode must not repeatedly stop and restart recording.
- Change the shortcut and mode during push-to-talk recording. The original
  shortcut release must finish it; the new binding applies to the next session.
  Repeat while toggled recording is active: its original shortcut must still
  stop it.
- Change the shortcut to a key that is already held. It must require release
  and a new press before starting another recording.
- Start recording, then use the tray's **Stop Dictation** action. Subsequent key
  release must not produce a second transcription. The action must also stop a
  recording started with **Start Dictation (Debug)**.
- Start with persistent capture enabled, begin recording, and disable persistent
  capture before stopping. After stop, the OS microphone indicator must turn
  off. Repeat with several audio settings changes during the same recording;
  the latest settings must apply afterward.
- During push-to-talk, disconnect the keyboard or sleep and wake the Mac. When
  input control returns, the old recording must be stopped; a fresh press must
  be required to start a new one. Verify listener recovery after a temporary
  input-monitoring interruption without relaunching ButterVoice.
- Begin a new dictation quickly while the previous one is processing. The app
  must finish the prior session before accepting another recording.

The debug log's `recording` entries contain session identifiers and start/stop
reasons even with verbose debug logging disabled. Use those entries to confirm
there is one accepted stop per recording and to distinguish a normal release,
missed-release recovery, listener interruption, and manual stop.

These are hardware acceptance checks; the presence of this checklist does not
indicate that sleep/wake, disconnects, or physical key sequences were exercised
by an automated test run.
