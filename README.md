# Joona

Free, open-source voice dictation for Windows and macOS. Hold **Ctrl+Win** (Windows) or **Ctrl+Cmd** (macOS) in any app, speak, and Joona types what you said where your cursor is.

- **Free and MIT licensed.** No account, no subscriptions, no word limits, no telemetry.
- **Local by default.** Transcription runs locally on your machine with Whisper (Metal-accelerated on Apple Silicon, Vulkan on Windows GPUs), Parakeet, or Moonshine in a separate `joona-engine` process. Your voice doesn't leave your computer unless you opt in to a cloud provider.
- **Speak in one language, type in another.** For example, speak Hindi and get English, or get Hinglish in Roman script.
- **Sarvam for Indian languages, only if you opt in.** Powered by `sarvam-105b` with reasoning control for translation and dictation. A consent screen explains what is sent, and a cloud badge shows whenever a dictation leaves your device.
- **Command Mode.** Select text, hold **Ctrl+Win+Alt** (Windows) or **Ctrl+Cmd+Option** (macOS) and say how to change it, or ask for something new to be written.
- **Bring your own keys** for OpenAI, Anthropic, Gemini, Groq, OpenRouter, Deepgram, ElevenLabs, Ollama, LM Studio or any OpenAI-compatible endpoint. Keys are kept securely in macOS Keychain or Windows Credential Manager.

## Install

Download the installer from the [latest release](https://github.com/saqib40/joona/releases/latest):

- **Windows:** Download `Joona_<version>_x64-setup.exe` and run it. It installs for your Windows account only, so it doesn't need administrator rights. Joona keeps itself up to date.
  - If Windows says *"Windows protected your PC"*, that build isn't code-signed yet. Click **More info**, then **Run anyway**.
- **macOS:** Download `Joona_<version>_aarch64.dmg` (Apple Silicon) and drag Joona to your Applications folder.
  - When opening for the first time, grant **Accessibility** (to type your text into active apps) and **Microphone** permissions in *System Settings › Privacy & Security*.

The first time Joona opens, it asks which languages you speak, checks your microphone, and downloads a speech model tailored to your hardware.

## Using Joona

| Action | Windows | macOS | Description |
| --- | --- | --- | --- |
| **Dictate** | Hold <kbd>Ctrl</kbd>+<kbd>Win</kbd> | Hold <kbd>Ctrl</kbd>+<kbd>Cmd</kbd> | Dictate while held, then type where your cursor is |
| **Hands-free** | Double-tap <kbd>Ctrl</kbd>+<kbd>Win</kbd>, or <kbd>Ctrl</kbd>+<kbd>Win</kbd>+<kbd>Space</kbd> | Double-tap <kbd>Ctrl</kbd>+<kbd>Cmd</kbd>, or <kbd>Ctrl</kbd>+<kbd>Cmd</kbd>+<kbd>Space</kbd> | Keep talking without holding keys; tap again to finish |
| **Cancel** | <kbd>Esc</kbd> | <kbd>Esc</kbd> | Cancel the current dictation |
| **Second language** | Hold <kbd>Ctrl</kbd>+<kbd>Win</kbd>+<kbd>Shift</kbd> | Hold <kbd>Ctrl</kbd>+<kbd>Cmd</kbd>+<kbd>Shift</kbd> | Dictate in your second output language (configured in Languages) |
| **Command Mode** | Hold <kbd>Ctrl</kbd>+<kbd>Win</kbd>+<kbd>Alt</kbd> | Hold <kbd>Ctrl</kbd>+<kbd>Cmd</kbd>+<kbd>Option</kbd> | Change selected text or generate new content |
| **Paste last** | <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | <kbd>Option</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd> | Paste your last dictation again |

You can customize every shortcut in **Settings › General**. For a full reference guide with your platform highlighted, see [Commands](https://saqib40.github.io/joona/commands.html).

## What leaves your device

- **Nothing you say**, unless you turn on a cloud provider such as Sarvam or configure your own API keys. Joona asks first and shows a cloud badge whenever audio or text is sent online.
- **Model downloads**, when you choose a speech model, from Hugging Face, OpenAI, GitHub, and the [Handy](https://github.com/cjpais/Handy) project's mirror.
- **Update checks.** About twice a day, Joona checks GitHub for newer releases. You can turn this off in *Settings › About*.

Joona has no accounts, no analytics, and no telemetry.

## Development

### Prerequisites
- **Node.js** 22+
- **Rust** (stable toolchain)
- **CMake**
- **Windows:** Windows 10/11, Visual Studio 2022+ with the C++ workload.
- **macOS:** macOS 12+, Xcode Command Line Tools (`xcode-select --install`).

### Getting Started

```bash
# Install dependencies
npm install

# Start Joona in dev mode (automatically builds the joona-engine sidecar)
npm run dev
```

To build or rebuild only the local engine sidecar:

```bash
npm run build:engine
```

To run all frontend and Rust tests:

```bash
npm test
```

Releases are built automatically by GitHub Actions. See [docs/releasing.md](docs/releasing.md).

## Repository layout

```
apps/desktop/            Tauri v2 desktop app: React UI (Hub and floating Bar) and Rust core
apps/web/                Static website and interactive keyboard shortcuts reference
crates/joona-engine/     Local speech-to-text engine sidecar (Whisper, Parakeet, Moonshine, Silero VAD)
crates/joona-protocol/   JSON-lines protocol shared by the desktop app and the engine sidecar
models/catalog.json      Downloadable speech model catalog with hashes, sizes, and licenses
scripts/                 Developer build, signing, and engine packaging scripts
docs/                    Documentation for release pipelines, updates, and models
```

## License

MIT. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
