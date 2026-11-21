import { toast } from "sonner";
import { create } from "zustand";
import {
  commands,
  type ModelCard,
  type ModelsOverview_Serialize as ModelsOverview,
  type Progress_Serialize as Progress,
} from "@/lib/bindings";

export type { ModelCard, ModelsOverview, Progress };

interface ModelsStore {
  overview: ModelsOverview | null;
  /** The latest progress of each download since Joona started. */
  downloads: Record<string, Progress>;
  error: string | null;
  refresh: () => Promise<void>;
  progress: (progress: Progress) => void;
  download: (id: string) => Promise<void>;
  cancel: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  use: (id: string | null, english: boolean) => Promise<void>;
}

export const useModels = create<ModelsStore>((set) => ({
  overview: null,
  downloads: {},
  error: null,
  refresh: async () => {
    try {
      const overview = await commands.modelsOverview();
      set((s) => ({
        overview,
        error: null,
        downloads: { ...s.downloads, ...Object.fromEntries(overview.downloads.map((p) => [p.id, p])) },
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },
  progress: (progress) => set((s) => ({ downloads: { ...s.downloads, [progress.id]: progress } })),
  download: async (id) => {
    try {
      await commands.modelDownload(id);
    } catch (e) {
      toast.error("Couldn't start the download", { description: String(e) });
    }
  },
  cancel: async (id) => {
    await commands.modelCancel(id);
  },
  remove: async (id) => {
    try {
      await commands.modelDelete(id);
      set((s) => {
        const downloads = { ...s.downloads };
        delete downloads[id];
        return { downloads };
      });
    } catch (e) {
      toast.error("Couldn't delete the model", { description: String(e) });
    }
  },
  use: async (id, english) => {
    try {
      await commands.modelUse(id, english);
      await useModels.getState().refresh();
    } catch (e) {
      toast.error("Couldn't switch models", { description: String(e) });
    }
  },
}));

export type DownloadState =
  | { kind: "idle" }
  | { kind: "running"; progress: Progress }
  | { kind: "paused"; progress: Progress }
  | { kind: "failed"; progress: Progress };

/** What the model's card shows about its download. */
export function downloadState(progress: Progress | undefined): DownloadState {
  if (!progress) return { kind: "idle" };
  switch (progress.stage) {
    case "done":
      return { kind: "idle" };
    case "cancelled":
      return progress.done > 0 ? { kind: "paused", progress } : { kind: "idle" };
    case "failed":
      return { kind: "failed", progress };
    default:
      return { kind: "running", progress };
  }
}

/** 0..1, or `null` while the size isn't known yet. */
export function fraction(progress: Progress): number | null {
  if (progress.stage === "converting") return progress.done / 1000;
  if (progress.stage === "verifying" || progress.stage === "unpacking") return null;
  return progress.total > 0 ? progress.done / progress.total : null;
}
