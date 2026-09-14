# Recording reliability checks

Automated regression coverage lives in `hotkey_macos`, `recording_control`, and
`audio`. Run the Rust suite and frontend build before testing a new app build.

## Live macOS verification

Use a test text field as the output destination. Verify the ButterVoice HUD and
the macOS microphone indicator separately. With persistent capture enabled, the
microphone indicator is expected to remain on between recordings.

- Quit ButterVoice and confirm it is no longer using the microphone. Relaunch
  with **Microphone Buffer** off, record once, and stop. The OS microphone
  indicator must turn off, and Control Center must stop listing ButterVoice as
  currently using the microphone (a recent-use entry is separate). Repeat several
  times with **System Default**, the current default microphone selected by name,
  and a named non-default microphone such as a headset or dock.
- With **Microphone Buffer** off, refresh the permission status and exercise the
  microphone permission request. After the probe completes, ButterVoice must not
  remain listed as currently using the microphone, including after a denied or
  failed request. Recheck after closing Audio settings, whose level meter
  temporarily enables the buffer.
- Enable **Microphone Buffer**, then disable it while idle. The OS microphone
  indicator must turn off. Repeat after changing between named microphones and
  after disconnecting an external microphone; subsequent recording must recover.
- With Right Option and push-to-talk selected, hold Right Option, hold Left
  Option, release Right Option, then release Left Option. Recording must end
  on the Right Option release. Repeat with the opposite order and the supported
  left/right Control and Command shortcuts.
- Exercise rapid taps and ordinary long holds. A physical press/release pair
  must produce at most one recording and one finalization.
- Hold Right Option for at least five seconds without pressing another key.
  Recording and the HUD must stay active until release; the watchdog must not
  log `release:MissedRelease` while it is held. Repeat for every modifier preset,
  including Fn, and confirm toggle mode still starts and stops on separate presses.
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

## cpal 0.15 teardown limitation

Input streams use a pause-on-drop owner because cpal 0.15.3's Core Audio
disconnect listener can retain the inner stream after ButterVoice releases its
handle. Explicitly pausing stops capture even when that reference cycle remains.
Selecting the current default microphone by name also reuses the default device
handle to avoid installing that listener. See [cpal #771](https://github.com/RustAudio/cpal/issues/771).

This mitigates continued capture; it does not free the retained stream resources
for non-default devices. A cpal upgrade incorporating the weak listener fix in
[#869](https://github.com/RustAudio/cpal/pull/869) remains a follow-up, as does
replacing the live permission probe with a native authorization-status query.
