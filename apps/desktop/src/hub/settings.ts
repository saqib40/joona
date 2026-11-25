import { toast } from "sonner";
import { commands } from "@/lib/bindings";
import { type Settings, useHub } from "./store";

let queue: Promise<void> = Promise.resolve();
let committed: Settings | null = null;

/**
 * Changes settings right away in the UI and saves them. If Rust refuses, the
 * old settings come back and a toast says why.
 */
export function saveSettings(change: (draft: Settings) => void): Promise<boolean> {
  const before = useHub.getState().settings;
  if (!before) return Promise.resolve(false);
  if (useHub.getState().saving === 0) {
    committed = before;
  }
  const draft = structuredClone(before);
  change(draft);
  useHub.setState((s) => ({ settings: draft, saving: s.saving + 1 }));

  const run = queue.then(async () => {
    const base = committed ?? before;
    const next = structuredClone(base);
    change(next);
    try {
      const saved = await commands.setSettings(next);
      committed = saved;
      useHub.setState((s) => ({
        saving: s.saving - 1,
        settings: s.saving === 1 ? saved : s.settings,
      }));
      return true;
    } catch (e) {
      useHub.setState((s) => ({
        saving: s.saving - 1,
        settings: s.saving === 1 ? base : s.settings,
      }));
      toast.error("Couldn't save that", { description: String(e) });
      return false;
    }
  });

  queue = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

/** Settings changed elsewhere, for example from the tray. */
export function receiveSettings(settings: Settings) {
  if (useHub.getState().saving > 0) return;
  committed = settings;
  useHub.setState({ settings });
}
