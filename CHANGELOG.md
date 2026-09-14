# buttervoice

## 0.1.1

### Patch Changes

- eb93a47: Fix stuck recording caused by overlapping modifier keys, repeated shortcut events, and lost shortcut releases. Recover interrupted shortcut listeners, preserve the active shortcut during settings changes, apply deferred microphone settings after recording, and add a Stop Dictation tray action with recording lifecycle diagnostics.
- ba830cb: Polish the recording HUD. The waveform now uses the butter palette, reacts to speech with fast attack and slow release, shimmers gently in silence, and fades into the pill ends. The pill has a warm glass background with a gradient ring and a glow that follows your voice, the switch to processing crossfades instead of cutting, and reduced-motion preferences are respected.
- 9cce8a8: Fix hold-to-record stopping immediately for modifier shortcuts such as Right Option. Check macOS modifier flags when detecting held keys so missed-release recovery does not stop an active hold.
- 209e3fc: Stop microphone capture before releasing recording and permission-check streams so the macOS microphone indicator clears when Microphone Buffer is off. Reuse the system default microphone handle when that microphone is selected by name.
