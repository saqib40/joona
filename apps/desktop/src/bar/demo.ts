import type { RouteLabel_Serialize as RouteLabel } from "@/lib/bindings";
import { emitLevel, useBar, type BarConfig, type BarState, type Toast } from "./store";

/**
 * Drives the bar outside the app, for designing it in a browser. `?scene=` pins
 * one state, for example `bar.html?scene=handsfree`; without it the states cycle.
 */
export function runDemo() {
  useBar.setState({ config: CONFIG, resting: true });
  startLevels();
  const params = new URLSearchParams(window.location.search);
  const wallpaper = { light: "#e9edf2", dark: "#1b212b" }[params.get("bg") ?? ""];
  document.body.style.background = wallpaper ?? "linear-gradient(140deg, #3f5f86, #b99a74)";
  const scene = params.get("scene");
  if (scene && scene in SCENES) {
    SCENES[scene]();
    return;
  }
  const order = ["resting", "listening", "processing", "done", "resting", "handsfree", "processing", "copied"];
  let i = 0;
  const next = () => {
    SCENES[order[i % order.length]]();
    i += 1;
    window.setTimeout(next, i % order.length === 1 ? 2500 : 1800);
  };
  next();
}

const CONFIG: BarConfig = {
  shortcut: "Ctrl+Win",
  languages: [
    { code: "hi", name: "Hindi", native: "हिन्दी" },
    { code: "ta", name: "Tamil", native: "தமிழ்" },
    { code: "en", name: "English", native: "English" },
  ],
  current: null,
  output: "en",
  outputName: "English",
  route: "Auto → EN",
  paused: false,
  theme: "system",
};

const ROUTE: RouteLabel = { text: "HI → EN", crossLanguage: true, cloud: null };

let toastId = 1;
const toast = (t: Omit<Toast, "id">) => useBar.getState().addToast({ ...t, id: toastId++ });

const show = (state: BarState) => useBar.getState().setState(state);
const listening = (handsFree: boolean, extra: Partial<Extract<BarState, { state: "listening" }>> = {}) =>
  show({ state: "listening", mode: "dictate", handsFree, elapsedMs: 0, route: ROUTE, endingSoon: false, ...extra });

const SCENES: Record<string, () => void> = {
  resting: () => show({ state: "resting" }),
  hover: () => {
    show({ state: "resting" });
    useBar.setState({ hover: true, pillHover: true });
  },
  paused: () => {
    show({ state: "resting" });
    useBar.setState({ config: { ...CONFIG, paused: true }, hover: true, pillHover: true });
  },
  listening: () => listening(false),
  handsfree: () => listening(true, { elapsedMs: 83_000 }),
  cloud: () => listening(true, { elapsedMs: 12_000, route: { ...ROUTE, cloud: "Sarvam" } }),
  endingsoon: () => listening(true, { elapsedMs: 19 * 60_000 + 7_000, endingSoon: true }),
  command: () => listening(false, { mode: "command", route: { text: "Command", crossLanguage: false, cloud: null } }),
  processing: () => show({ state: "processing", route: ROUTE }),
  done: () => show({ state: "done", outcome: { kind: "pasted" } }),
  copied: () => {
    show({ state: "resting" });
    toast({
      tone: "neutral",
      title: "Copied, press Ctrl+V",
      detail: "Slack didn't accept pasted text.",
      actions: [{ action: "alwaysType", label: "Always type in Slack" }],
      timeoutMs: 6000,
    });
  },
  toasts: () => {
    show({ state: "resting" });
    toast({
      tone: "neutral",
      title: "Cancelled",
      detail: null,
      actions: [{ action: "undo", label: "Undo" }],
      timeoutMs: 600_000,
    });
    toast({
      tone: "error",
      title: "Couldn't transcribe",
      detail: "Sarvam didn't respond. Your audio is kept for a minute.",
      actions: [
        { action: "retry", label: "Retry" },
        { action: "useLocal", label: "Use local" },
      ],
      timeoutMs: 600_000,
    });
  },
  menu: () => {
    show({ state: "resting" });
    useBar.setState({ hover: true, menu: "context" });
  },
  language: () => {
    show({ state: "resting" });
    useBar.setState({ hover: true, menu: "language" });
  },
};

/** A voice-like level: syllables of 150–350 ms with short gaps. */
function startLevels() {
  let syllableEnds = 0;
  let loud = 0;
  window.setInterval(() => {
    const now = performance.now();
    if (now > syllableEnds) {
      loud = Math.random() < 0.18 ? 0.04 : 0.35 + Math.random() * 0.6;
      syllableEnds = now + 150 + Math.random() * 200;
    }
    emitLevel(loud * (0.85 + Math.random() * 0.3));
  }, 33);
}
