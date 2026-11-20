# Changelog

Each release's notes come from its section here. Joona shows the first line
when it offers the update, so keep that line to one plain sentence.

## 0.2.2

Adds macOS support and cross-platform releases for Windows and macOS.

- Joona is now available natively on macOS with Apple Silicon (M-series) and Intel support.
- Non-activating floating bar that stays visible across full-screen spaces and desktops.
- Full keyboard accessibility and global shortcut support (Ctrl+Cmd).
- High-performance local Metal acceleration for Whisper models.
- Signed and notarized installer (.dmg) and updater builds for macOS alongside Windows installers.

## 0.2.1

Fixes the Joona window sometimes opening blank.

- On some PCs, especially the first time after installing, the Joona window opened with only its title bar, so setup never appeared. The window now waits until Joona is ready.

## 0.2.0

Better model suggestions on PCs with a graphics card.

- The suggested model only counts your graphics card while "Use the graphics card" is on, since models run on the CPU otherwise. Before, some PCs were offered models too slow for their CPU.

## 0.1.0

The first release of Joona: free, open-source dictation for Windows that runs on your PC.

- Hold Ctrl+Win in any app, speak, and Joona types what you said. Double-tap Ctrl+Win, or press Ctrl+Win+Space, for hands-free.
- Speech recognition on your PC with Whisper, Parakeet or Moonshine, using the graphics card when it helps.
- Speak in one language and type in another, with an extra shortcut (Ctrl+Win+Shift) for a second output language.
- Sarvam for harder Indian languages, only if you turn it on.
- Command Mode: select text, hold Ctrl+Win+Alt and say how to change it, or ask for something new.
- AI polish, dictionary, snippets and styles per app.
- Bring your own keys for OpenAI, Anthropic, Gemini, Groq, OpenRouter, Deepgram, ElevenLabs, or any OpenAI-compatible server, or run a local model.
