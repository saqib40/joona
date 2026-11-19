import { create } from "zustand";
import { commands, type UpdateStatus_Serialize as UpdateStatus } from "@/lib/bindings";

export type { UpdateStatus };

interface UpdatesStore {
  status: UpdateStatus | null;
  /** From Rust, as a check or download moves on. */
  receive: (status: UpdateStatus) => void;
  refresh: () => Promise<void>;
  check: () => Promise<void>;
  /** Downloads and installs the update. Joona closes and opens again. */
  install: () => Promise<void>;
}

export const useUpdates = create<UpdatesStore>((set) => ({
  status: null,
  receive: (status) => set({ status }),
  refresh: async () => {
    try {
      set({ status: await commands.updateStatus() });
    } catch {
      // The About page keeps showing what it had.
    }
  },
  check: async () => {
    try {
      set({ status: await commands.updateCheck() });
    } catch (e) {
      set({ status: { state: "failed", message: String(e) } });
    }
  },
  install: async () => {
    try {
      await commands.updateInstall();
    } catch (e) {
      set({ status: { state: "failed", message: String(e) } });
    }
  },
}));
