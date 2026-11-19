import { toast } from "sonner";
import { create } from "zustand";
import {
  type AiChoice,
  type AiOverview_Serialize as AiOverview,
  type AiProvider_Serialize as AiProvider,
  type AiSample,
  commands,
  type LocalAiInfo_Serialize as LocalAiInfo,
  type ProviderFailure,
} from "@/lib/bindings";
import { asFailure } from "./providers";
import { saveSettings } from "./settings";
import { type Settings, useHub } from "./store";

export { aiConsent } from "./providers";
export type { AiChoice, AiOverview, AiProvider, AiSample, LocalAiInfo };

/** Joona's own model, which runs on this PC. */
export const LOCAL_AI = "local";
/** Its download, as `ModelDownload` events name it. */
export const LOCAL_AI_DOWNLOAD = "local-ai";

interface AiStore {
  overview: AiOverview | null;
  error: string | null;
  /** The last sample, and the model that wrote it. */
  sample: { by: string; result: AiSample } | null;
  testing: boolean;
  refresh: () => Promise<void>;
  /** Runs a sample dictation through the model in use. */
  test: () => Promise<ProviderFailure | null>;
  /** Has Smart polish use `provider`, with `apply` saved alongside. */
  use: (provider: string, apply?: (draft: Settings) => void) => Promise<boolean>;
  /** Stops using an AI model; Smart polish falls back to Basic. */
  stop: () => Promise<void>;
  download: () => Promise<void>;
  cancel: () => Promise<void>;
  remove: () => Promise<void>;
}

let asked = 0;

export const useAi = create<AiStore>((set, get) => ({
  overview: null,
  error: null,
  sample: null,
  testing: false,
  refresh: async () => {
    const ask = ++asked;
    try {
      const overview = await commands.aiOverview();
      if (ask === asked) set({ overview, error: null });
    } catch (e) {
      if (ask === asked) set({ error: String(e) });
    }
  },
  test: async () => {
    set({ testing: true });
    try {
      const result = await commands.aiTest();
      const ready = get().overview?.ready;
      set({ sample: { by: ready ? choiceKey(ready) : "", result } });
      return null;
    } catch (e) {
      return asFailure(e);
    } finally {
      set({ testing: false });
    }
  },
  use: async (provider, apply) => {
    const switching = useHub.getState().settings?.polish.provider !== provider;
    const saved = await saveSettings((s) => {
      s.polish.provider = provider;
      if (switching) {
        s.polish.model = null;
        s.polish.endpoint = null;
        if (s.polish.level === "basic") s.polish.level = "smart";
      }
      apply?.(s);
    });
    if (!saved) return false;
    set({ sample: null });
    await get().refresh();
    return true;
  },
  stop: async () => {
    const saved = await saveSettings((s) => {
      s.polish.provider = null;
      s.polish.model = null;
      s.polish.endpoint = null;
      if (s.polish.level === "smart") s.polish.level = "basic";
    });
    if (saved) await get().refresh();
  },
  download: async () => {
    try {
      await commands.localAiDownload();
    } catch (e) {
      toast.error("Couldn't start the download", { description: String(e) });
    }
  },
  cancel: async () => {
    await commands.localAiCancel();
  },
  remove: async () => {
    try {
      await commands.localAiDelete();
    } catch (e) {
      toast.error("Couldn't delete the local AI model", { description: String(e) });
    }
    await get().refresh();
  },
}));

/** Which model a sample came from, so an old one isn't shown for a new model. */
export const choiceKey = (choice: AiChoice) => `${choice.provider}/${choice.model}`;

/** The model's name as people read it: Joona's own model by name, others by their ID. */
export const modelLabel = (choice: AiChoice, overview: AiOverview | null) =>
  choice.provider === LOCAL_AI ? (overview?.local?.name ?? choice.model) : choice.model;

/** Whether `url` points at this PC. Mirrors `llm::is_local_url`. */
export function isLocalUrl(url: string): boolean {
  try {
    const host = new URL(url.includes("://") ? url : `http://${url}`).hostname.toLowerCase();
    return host === "localhost" || host.endsWith(".localhost") || host === "[::1]" || host.startsWith("127.");
  } catch {
    return false;
  }
}

/** Whether what the user typed could be a server's address. */
export function looksLikeAddress(typed: string): boolean {
  try {
    const url = new URL(typed.includes("://") ? typed : `http://${typed}`);
    return (url.protocol === "http:" || url.protocol === "https:") && url.hostname !== "";
  } catch {
    return false;
  }
}

/** Whether text sent to `provider` at `endpoint` leaves this PC. Mirrors `llm::sends_away`. */
export function sendsAway(provider: AiProvider, endpoint: string | null): boolean {
  if (!provider.endpoint) return !provider.local;
  return !isLocalUrl(endpoint?.trim() || provider.defaultEndpoint || "");
}
