import { commands, events } from "@/lib/bindings";
import { isTauri } from "@/lib/utils";
import { emitLevel, useBar } from "./store";

let connected: Promise<void> | null = null;

/** Loads the bar's state from Rust and follows its events. Safe to call more than once. */
export function connect(): Promise<void> {
  connected ??= listen();
  return connected;
}

async function listen() {
  if (!isTauri()) {
    const { runDemo } = await import("./demo");
    runDemo();
    return;
  }
  const bar = useBar.getState();
  await Promise.all([
    events.barStateChanged.listen((e) => useBar.getState().setState(e.payload)),
    events.micLevel.listen((e) => emitLevel(e.payload ?? 0)),
    events.toastShown.listen((e) => useBar.getState().addToast(e.payload)),
    events.toastDismissed.listen((e) => useBar.getState().removeToast(e.payload)),
    events.barHover.listen((e) => useBar.getState().setHover(e.payload)),
    events.barResting.listen((e) => useBar.setState({ resting: e.payload })),
    events.barConfigChanged.listen((e) => useBar.setState({ config: e.payload })),
  ]);
  const snapshot = await commands.barSnapshot();
  bar.setState(snapshot.state);
  useBar.setState({ config: snapshot.config, resting: snapshot.resting, snoozed: snapshot.snoozed });
}

/** Commands that do nothing outside the app, so the bar can be previewed in a browser. */
export const bar = {
  start: () => call(() => commands.barStart()),
  stop: () => call(() => commands.barStop()),
  cancel: () => call(() => commands.barCancel()),
  snooze: (snooze: boolean) => {
    useBar.setState({ snoozed: snooze });
    return call(() => commands.barSnooze(snooze));
  },
  toastAction: (toast: number, action: Parameters<typeof commands.toastAction>[1]) =>
    call(() => commands.toastAction(toast, action)),
  openHub: (page: string | null = null) => call(() => commands.openHub(page)),
  setLanguage: (code: string | null) => call(() => commands.setLanguage(code)),
  setPaused: (paused: boolean) => call(() => commands.setPaused(paused)),
  setRegions: (regions: Parameters<typeof commands.barSetRegions>[0]) => call(() => commands.barSetRegions(regions)),
};

async function call(run: () => Promise<unknown>) {
  if (!isTauri()) return;
  try {
    await run();
  } catch (e) {
    console.error(e);
  }
}
