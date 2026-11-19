import { mockIPC } from "@tauri-apps/api/mocks";
import type { Language } from "@/lib/bindings";
import { defaultSettings } from "./demo";
import { useProviders } from "./providers";
import { type Settings, useHub } from "./store";

export const LANGUAGES: Language[] = [
  { code: "en", name: "English", native: "English", whisper: true, local: "good", sarvam: true, latin: true },
  { code: "hi", name: "Hindi", native: "हिन्दी", whisper: true, local: "fair", sarvam: true, latin: false },
  { code: "ta", name: "Tamil", native: "தமிழ்", whisper: true, local: "fair", sarvam: true, latin: false },
  { code: "or", name: "Odia", native: "ଓଡ଼ିଆ", whisper: false, local: "none", sarvam: true, latin: false },
  { code: "fr", name: "French", native: "Français", whisper: true, local: "good", sarvam: false, latin: true },
];

type Answer = (cmd: string, args: Record<string, unknown>) => unknown;

/**
 * Loads the Hub with default settings and `change` applied, and fakes Rust.
 * `answer` goes first; returning `undefined` from it falls back to the defaults.
 * Returns every settings object saved, newest last.
 */
export function fakeHub(change?: (s: Settings) => void, answer?: Answer): Settings[] {
  const settings = defaultSettings();
  change?.(settings);
  useHub.setState({ settings, saving: 0, page: "models" });
  useProviders.setState({ keys: [] });
  const saved: Settings[] = [];
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      const answered = answer?.(cmd, args);
      if (answered !== undefined) return answered;
      if (cmd === "languages") return LANGUAGES;
      if (cmd === "provider_keys") return [];
      if (cmd !== "set_settings") return null;
      saved.push(args.settings as Settings);
      return args.settings;
    },
    { shouldMockEvents: true },
  );
  return saved;
}
