import { toast } from "sonner";
import { commands } from "@/lib/bindings";
import { type Settings, useHub } from "./store";

/**
 * Changes settings right away in the UI and saves them. If Rust refuses, the
 * old settings come back and a toast says why.
 */
export async function saveSettings(change: (draft: Settings) => void): Promise<boolean> {
  const before = useHub.getState().settings;
  if (!before) return false;
  const draft = structuredClone(before);
  change(draft);
  useHub.setState((s) => ({ settings: draft, saving: s.saving + 1 }));
  try {
    const saved = await commands.setSettings(draft);
    useHub.setState((s) => ({ saving: s.saving - 1, settings: s.saving === 1 ? saved : s.settings }));
    return true;
  } catch (e) {
    useHub.setState((s) => ({ saving: s.saving - 1, settings: before }));
    toast.error("Couldn't save that", { description: String(e) });
    return false;
  }
}

/** Settings changed elsewhere, for example from the tray. */
export function receiveSettings(settings: Settings) {
  if (useHub.getState().saving > 0) return;
  useHub.setState({ settings });
}
