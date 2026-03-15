# ButterVoice

A tray-first local dictation app for macOS. Record speech with a hotkey, transcribe it offline using [whisper.cpp](https://github.com/ggerganov/whisper.cpp), and inject the text directly into whatever input field is focused.

Built with [Tauri 2](https://v2.tauri.app/), React 19, and Rust.

## How It Works

1. Press and hold a hotkey (default: Right Option) to record from your microphone
2. Release the key — audio is sent to a local Whisper model for transcription
3. The transcribed text is typed into whatever text field is currently focused

Everything runs locally. No audio leaves your machine unless you opt into the optional LLM cleanup feature.

## Features

- **Push-to-talk or toggle mode** dictation via configurable global hotkey
- **Offline transcription** using whisper.cpp with Metal GPU acceleration on Apple Silicon
- **Text injection** directly into the focused input field (or clipboard fallback)
- **Post-processing pipeline** — spell correction, punctuation repair, truecasing, number normalization, grammar rules
- **Optional LLM cleanup** via any OpenAI-compatible API (OpenRouter, local models, etc.)
- **Model management** — download and switch between Whisper models from tiny to large-v3
- **Audio controls** — device selection, channel mode, high-pass filter, input gain
- **Transcript history** with raw and cleaned text
- **Launch at login** and tray-only operation

## Requirements

- macOS 13.0+ (Ventura or later)
- Apple Silicon (aarch64) or Intel (x86_64)
- Microphone access, Accessibility, and Input Monitoring permissions

### Build Prerequisites

- [Rust](https://rustup.rs/) (via rustup)
- [Node.js](https://nodejs.org/)
- [pnpm](https://pnpm.io/)

## Getting Started

### Install from Source

```bash
# Clone the repository
git clone <repo-url>
cd ButterVoice

# Install frontend dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Or build and install to /Applications
./scripts/build-install.sh
```

The build script will:
1. Check prerequisites (rustc, cargo, node, pnpm)
2. Install dependencies and build the app
3. Copy `ButterVoice.app` to `/Applications`
4. Reset macOS TCC permissions so the app re-prompts on launch

### First Launch

1. Open ButterVoice — it appears as a tray icon in the menu bar
2. Click the tray icon to open Settings
3. Grant the requested permissions (Microphone, Accessibility, Input Monitoring)
4. Download a Whisper model (the default `base.en-q5_1` at 57 MB is a good starting point)
5. Focus a text field anywhere, hold Right Option, speak, and release

## Whisper Models

Models are downloaded from HuggingFace and cached locally. Quantized variants (`q5_0`, `q5_1`, `q8_0`) are smaller and faster with minimal quality loss.

| Model | Size | Notes |
|---|---|---|
| `tiny.en-q5_1` | 31 MB | Fastest, lowest quality |
| `tiny.en` | 75 MB | |
| `base.en-q5_1` | 57 MB | **Default — recommended starting point** |
| `base.en` | 142 MB | |
| `small.en-q5_1` | 181 MB | Good quality/speed balance |
| `small.en` | 466 MB | |
| `medium.en` | 1.5 GB | High quality, slower |
| `large-v3-turbo-q5_0` | 547 MB | Multilingual, good speed |
| `large-v3-turbo` | 1.5 GB | Multilingual |
| `large-v3-q5_0` | 1.1 GB | Best quality, multilingual |
| `large-v3` | 3.1 GB | Best quality, slowest |

Models ending in `.en` are English-only and generally perform better for English dictation. The `large-v3` family supports multiple languages (English, Spanish, French, German, Japanese, Chinese, and more).

## Settings

### General
- **Hotkey** — Right Option (default), Left Option, Right Command, Right/Left Control, Fn, or a custom keycode
- **Dictation mode** — Push-to-talk (hold to record) or Toggle (press to start/stop)
- **Launch at login**
- **Debug logging**

### Audio
- **Microphone** — select from available input devices
- **Channel mode** — Left, Right, or Mono Mix
- **High-pass filter** — Off, 80 Hz, or 120 Hz (reduces low-frequency rumble)
- **Input gain** — -24 to +24 dB
- **Persistent mic stream** — keeps the microphone open between recordings for faster start

### Whisper Tuning
- **Language** — English and 10+ other languages
- **Compute mode** — Auto, CPU, or GPU
- **Beam size** — 1-10 (higher = more accurate, slower)
- **Temperature** and temperature increment for sampling
- **Speech detection threshold** — filters silence
- **Thread count** — 0 for auto

### Text Processing (Local Pipeline)

A multi-stage pipeline that cleans up Whisper output without any network calls:

1. **Sentence segmentation** — adds sentence boundaries
2. **Punctuation repair** — fixes misplaced punctuation
3. **Truecasing** — corrects capitalization of proper nouns
4. **Inverse text normalization** — converts spoken numbers to digits ("twenty three" to "23")
5. **Spell correction** — dictionary-based with custom word support
6. **Grammar rules** — conservative rule-based corrections

Safety guards (confidence threshold and max edit ratio) prevent the pipeline from making overly aggressive changes.

### AI Enhancement (Optional)

Connect to any OpenAI-compatible API for LLM-based text cleanup after local processing:
- Configurable base URL, API key, and model
- Custom system prompt support
- Works with OpenRouter, Ollama, LM Studio, or any compatible endpoint

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  Frontend (React 19 + TypeScript)                    │
│  ├── Mantine UI components                           │
│  ├── TanStack Router (file-based routing)            │
│  ├── TanStack React Query (server state)             │
│  └── Jotai (client state)                            │
├──────────────────────────────────────────────────────┤
│  Tauri IPC (invoke / listen)                         │
├──────────────────────────────────────────────────────┤
│  Backend (Rust)                                      │
│  ├── Audio capture (cpal) → WAV → whisper.cpp        │
│  ├── Post-processing pipeline                        │
│  ├── Text injection (enigo + arboard)                │
│  ├── Settings persistence (tauri-plugin-store)       │
│  └── macOS integration (cocoa, dock, tray, hotkeys)  │
└──────────────────────────────────────────────────────┘
```

### Audio Pipeline

Microphone audio is captured via `cpal`, processed through channel selection, optional high-pass filtering, gain adjustment, and resampled to 16 kHz (Whisper's required sample rate) with anti-alias filtering. Output is written as a 16-bit mono WAV file.

When persistent mic mode is enabled, a ring buffer keeps a 600ms preroll so recording starts capture slightly before the hotkey is pressed.

### Transcription

whisper.cpp runs as a bundled native binary (pre-built for both aarch64 and x86_64). On Apple Silicon, Metal GPU acceleration is used by default. The app manages the binary lifecycle and passes audio files for transcription.

### Text Injection

Transcribed text is injected into the focused input field using macOS accessibility APIs (via `enigo`). If injection fails, text is copied to the clipboard as a fallback.

## Project Structure

```
ButterVoice/
├── src/                          # Frontend (React + TypeScript)
│   ├── routes/                   # TanStack file-based routes
│   │   ├── dashboard/            # Main dashboard views
│   │   ├── settings/             # Settings pages (general, audio, models, etc.)
│   │   └── debug/                # Debug log viewer
│   ├── components/               # UI components
│   ├── stores/                   # Jotai state atoms
│   ├── lib/                      # Tauri commands, hooks, utilities
│   └── types/                    # TypeScript type definitions
├── src-tauri/                    # Backend (Rust)
│   ├── src/
│   │   ├── lib.rs                # Tauri commands and app setup
│   │   ├── audio.rs              # Audio capture and processing
│   │   ├── whisper.rs            # whisper.cpp integration
│   │   ├── settings.rs           # Settings schema and persistence
│   │   ├── state.rs              # Application state coordinator
│   │   ├── hotkey.rs             # Global hotkey listener
│   │   ├── injector.rs           # Text injection into focused fields
│   │   └── post_process/         # Text processing pipeline modules
│   └── resources/
│       ├── whispercpp/           # Bundled whisper.cpp binaries
│       └── dictionaries/         # Spell-check dictionaries
├── scripts/
│   └── build-install.sh          # Build and install to /Applications
└── package.json
```

## Development

```bash
# Start the dev server (frontend hot-reload + Rust backend)
pnpm tauri dev

# Build the frontend only
pnpm build

# Build the full app bundle
pnpm tauri build
```

### IDE Setup

- [VS Code](https://code.visualstudio.com/) with [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
