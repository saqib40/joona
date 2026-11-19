import { toast } from "sonner";
import { create } from "zustand";
import { commands, type ProviderFailure } from "@/lib/bindings";
import { saveSettings } from "./settings";
import { type Settings, useHub } from "./store";

export const SARVAM = "sarvam";
export const SARVAM_PRIVACY = "https://www.sarvam.ai/privacy-policy";
export const SARVAM_KEYS = "https://dashboard.sarvam.ai/key-management";

interface ProvidersStore {
  /** Providers with a saved key, or `null` until Rust says. */
  keys: string[] | null;
  refresh: () => Promise<void>;
  /**
   * Checks `key` with the provider and saves it. A key the provider refuses
   * isn't saved; that failure comes back. Other problems, such as being
   * offline, don't stop the save. `endpoint` is the address of a server the
   * user runs.
   */
  save: (provider: string, key: string, endpoint?: string | null) => Promise<ProviderFailure | null>;
  /** Checks the saved key. */
  test: (provider: string) => Promise<ProviderFailure | null>;
  /** Deletes the key and stops sending anything to the provider. */
  remove: (provider: string) => Promise<void>;
}

export const useProviders = create<ProvidersStore>((set, get) => ({
  keys: null,
  refresh: async () => {
    try {
      set({ keys: await commands.providerKeys() });
    } catch (e) {
      set((s) => ({ keys: s.keys ?? [] }));
      toast.error("Couldn't read your API keys", { description: String(e) });
    }
  },
  save: async (provider, key, endpoint = null) => {
    const failure = await commands.providerTest(provider, key, endpoint).then(
      () => null,
      (e: unknown) => asFailure(e),
    );
    if (failure?.keyProblem) return failure;
    try {
      await commands.providerKeySet(provider, key);
    } catch (e) {
      return { message: `Couldn't save the key: ${String(e)}`, keyProblem: false };
    }
    await get().refresh();
    if (failure) toast.warning("Saved, but Joona couldn't check the key", { description: failure.message });
    return null;
  },
  test: (provider) =>
    commands.providerTest(provider, null, null).then(
      () => null,
      (e: unknown) => asFailure(e),
    ),
  remove: async (provider) => {
    try {
      await commands.providerKeyDelete(provider);
    } catch (e) {
      toast.error("Couldn't delete the key", { description: String(e) });
      return;
    }
    await get().refresh();
    await saveSettings((s) => {
      const consents = [provider, aiConsent(provider), sttConsent(provider)];
      s.privacy.consented = s.privacy.consented.filter((p) => !consents.includes(p));
      if (s.polish.provider === provider) {
        s.polish.provider = null;
        s.polish.model = null;
        s.polish.endpoint = null;
      }
      if (s.transcription.provider === provider) {
        s.transcription = { provider: null, model: null, endpoint: null };
      }
    });
  },
}));

/** What `privacy.consented` lists once the user agreed to send text to the provider's AI model. */
export const aiConsent = (provider: string) => `ai:${provider}`;

/** What `privacy.consented` lists once the user agreed to send the provider their recordings. */
export const sttConsent = (provider: string) => `stt:${provider}`;

/** Rust rejects with a `ProviderFailure`; anything else becomes one. */
export function asFailure(error: unknown): ProviderFailure {
  if (typeof error === "object" && error !== null && "message" in error && "keyProblem" in error) {
    return error as ProviderFailure;
  }
  return { message: String(error), keyProblem: false };
}

export const consentedTo = (settings: Settings | null, provider: string) =>
  settings?.privacy.consented.includes(provider) ?? false;

/** Whether the user agreed to Sarvam, and whether its key is saved. */
export function useSarvam(): { consented: boolean; hasKey: boolean; ready: boolean } {
  const consented = useHub((s) => consentedTo(s.settings, SARVAM));
  const hasKey = useProviders((s) => s.keys?.includes(SARVAM) ?? false);
  return { consented, hasKey, ready: consented && hasKey };
}
