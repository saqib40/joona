import { create } from "zustand";
import {
  commands,
  type SttChoice,
  type SttOverview_Serialize as SttOverview,
  type SttProvider_Serialize as SttProvider,
} from "@/lib/bindings";
import { isLocalUrl } from "./ai";
import { saveSettings } from "./settings";
import type { Settings } from "./store";

export { sttConsent } from "./providers";
export type { SttChoice, SttOverview, SttProvider };

interface SttStore {
  overview: SttOverview | null;
  error: string | null;
  refresh: () => Promise<void>;
  /** Has `provider` hear dictations, with `apply` saved alongside. */
  use: (provider: string, apply?: (draft: Settings) => void) => Promise<boolean>;
  /** Dictations go back to the model on this PC. */
  stop: () => Promise<void>;
}

let asked = 0;

export const useStt = create<SttStore>((set, get) => ({
  overview: null,
  error: null,
  refresh: async () => {
    const ask = ++asked;
    try {
      const overview = await commands.sttOverview();
      if (ask === asked) set({ overview, error: null });
    } catch (e) {
      if (ask === asked) set({ error: String(e) });
    }
  },
  use: async (provider, apply) => {
    const saved = await saveSettings((s) => {
      s.transcription = { provider, model: null, endpoint: null };
      apply?.(s);
    });
    if (saved) await get().refresh();
    return saved;
  },
  stop: async () => {
    const saved = await saveSettings((s) => {
      s.transcription = { provider: null, model: null, endpoint: null };
    });
    if (saved) await get().refresh();
  },
}));

/** Whether recordings sent to `provider` at `endpoint` leave this PC. Mirrors `stt::sends_away`. */
export function sendsAway(provider: SttProvider, endpoint: string | null): boolean {
  return !provider.endpoint || !isLocalUrl(endpoint?.trim() ?? "");
}
