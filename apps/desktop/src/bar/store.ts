import { create } from "zustand";
import type {
  BarConfig_Serialize as BarConfig,
  BarState_Serialize as BarState,
  Toast_Serialize as Toast,
} from "@/lib/bindings";

export type { BarConfig, BarState, Toast };

export type Menu = null | "context" | "language";

/** Toasts beyond this many push the oldest out. Two fit the window with their buttons. */
const MAX_TOASTS = 2;

interface BarStore {
  state: BarState;
  /** `performance.now()` minus the dictation's elapsed time, for the timer. */
  startedAt: number;
  config: BarConfig | null;
  /** Whether the resting pill shows. */
  resting: boolean;
  snoozed: boolean;
  /** The cursor is over an interactive part of the bar, as Rust sees it. */
  hover: boolean;
  /** The cursor is over the pill itself. */
  pillHover: boolean;
  toasts: Toast[];
  menu: Menu;
  setState: (state: BarState) => void;
  setHover: (hover: boolean) => void;
  setPillHover: (hover: boolean) => void;
  addToast: (toast: Toast) => void;
  removeToast: (id: number) => void;
  setMenu: (menu: Menu) => void;
}

export const useBar = create<BarStore>((set) => ({
  state: { state: "resting" },
  startedAt: performance.now(),
  config: null,
  resting: true,
  snoozed: false,
  hover: false,
  pillHover: false,
  toasts: [],
  menu: null,
  setState: (state) =>
    set((s) => ({
      state,
      startedAt: state.state === "listening" ? performance.now() - state.elapsedMs : s.startedAt,
      menu: state.state === "resting" ? s.menu : null,
    })),
  // The webview gets no pointerleave once the window turns click-through again.
  setHover: (hover) => set((s) => ({ hover, pillHover: hover && s.pillHover })),
  setPillHover: (pillHover) => set({ pillHover }),
  addToast: (toast) =>
    set((s) => ({
      toasts: [...s.toasts.filter((t) => t.id !== toast.id), toast].slice(-MAX_TOASTS),
    })),
  removeToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  setMenu: (menu) => set({ menu }),
}));

type LevelListener = (level: number) => void;
const levelListeners = new Set<LevelListener>();

/** Microphone levels arrive about 30 times a second, so they skip React state. */
export function onLevel(listener: LevelListener): () => void {
  levelListeners.add(listener);
  return () => levelListeners.delete(listener);
}

export function emitLevel(level: number) {
  for (const listener of levelListeners) listener(level);
}
