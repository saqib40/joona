# Joona

Free, open-source voice dictation for Windows. Hold **Ctrl+Win** in any app, speak, and Joona types what you said where your cursor is.

- **Free and MIT licensed.** No account, no word limits.
- **Local by default.** Transcription runs on your PC with Whisper (whisper.cpp), Parakeet or Moonshine in a separate `joona-engine` process. Your voice doesn't leave the PC unless you turn on a cloud provider.
- **Speak in one language, type in another.** For example, speak Hindi and get English, or get Hinglish in Roman script.
- **Sarvam for harder Indian languages, only if you opt in.** A consent screen explains what is sent, and a cloud badge shows whenever a dictation leaves the PC.
- **Command Mode.** Select text, hold Ctrl+Win+Alt and say how to change it, or ask for something new to be written.
- **Bring your own keys** for OpenAI, Anthropic, Gemini, Groq, OpenRouter, Deepgram, ElevenLabs, Ollama, LM Studio or any OpenAI-compatible endpoint. Keys are stored in Windows Credential Manager.

## Install

Download `Joona_<version>_x64-setup.exe` from the [latest release](https://github.com/saqib40/joona/releases/latest) and run it. It installs for your Windows account only, so it doesn't need administrator rights. Joona keeps itself up to date.

If Windows says "Windows protected your PC", that build isn't code-signed yet. Click **More info**, then **Run anyway**.

The first time Joona opens, it asks which languages you speak, checks your microphone and downloads a speech model that suits your PC.

## Using Joona

| Shortcut | What it does |
| --- | --- |
| Hold **Ctrl+Win** | Dictate while held, then type it where your cursor is |
| Double-tap **Ctrl+Win**, or **Ctrl+Win+Space** | Hands-free: keep talking without holding keys, and tap Ctrl+Win to finish |
| **Esc** | Cancel the dictation |
| Hold **Ctrl+Win+Shift** | Dictate in your second output language, if you set one in Languages |
| Hold **Ctrl+Win+Alt** | Command Mode: change the selected text, or write something new |
| **Alt+Shift+Z** | Paste your last dictation again |

You can change the shortcuts in Settings.

## What leaves your PC

- **Nothing you say**, unless you turn on a cloud provider such as Sarvam or one of your own keys. Joona asks first, and shows a cloud badge whenever a dictation is sent.
- **Model downloads**, when you choose a model, from Hugging Face, OpenAI, GitHub and the [Handy](https://github.com/cjpais/Handy) project's mirror.
- **Update checks.** About twice a day, Joona asks GitHub whether there's a new version. You can turn this off in Settings › About.

Joona has no accounts, analytics or telemetry.

## Development

Requirements: Windows 10 or 11, Node.js 22+, Rust (MSVC), Visual Studio 2022+ with the C++ workload, and CMake (the one bundled with Visual Studio works).

```powershell
.\scripts\setup-dev.ps1     # installs missing tools, npm packages, builds the engine sidecar
npm run dev                 # starts Joona with hot reload
```

To rebuild only the engine sidecar:

```powershell
.\scripts\build-engine.ps1            # debug, CPU
.\scripts\build-engine.ps1 -Release   # release, CPU
```

Releases are built by GitHub Actions. See [docs/releasing.md](docs/releasing.md).

## Repository layout

```
apps/desktop/            Tauri v2 app: React UI (Hub and Joona Bar) and the Rust core
crates/joona-engine/     Local speech-to-text sidecar (Whisper, Parakeet, Moonshine, Silero VAD)
crates/joona-protocol/   JSON-lines protocol shared by the app and the engine
models/catalog.json      Downloadable models with sizes, hashes and licences
scripts/                 Developer and release scripts
docs/                    How releases and updates work
```

## Licence

MIT. See [LICENSE](LICENSE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
