import { commands, events } from "@/lib/bindings";
import { useAi } from "./ai";
import { useHistory } from "./history";
import { useModels } from "./models";
import { useProviders } from "./providers";
import { receiveSettings } from "./settings";
import { useHub } from "./store";
import { useStt } from "./stt";
import { applyMica } from "./theme";
import { useUpdates } from "./updates";

let connected: Promise<void> | null = null;

/** Loads what the Hub shows and follows Rust's events. Safe to call more than once. */
export function connect(): Promise<void> {
  connected ??= listen();
  return connected;
}

async function listen() {
  await Promise.all([
    events.settingsChanged.listen((e) => receiveSettings(e.payload)),
    events.hubNavigate.listen((e) => useHub.getState().navigate(e.payload)),
    events.historyAdded.listen((e) => useHistory.getState().added(e.payload)),
    events.historyChanged.listen(() => {
      useHistory.setState((s) => ({ version: s.version + 1 }));
      void useHistory.getState().reload();
    }),
    events.modelDownload.listen((e) => useModels.getState().progress(e.payload)),
    events.modelsChanged.listen(() => void useModels.getState().refresh()),
    events.aiChanged.listen(() => void useAi.getState().refresh()),
    events.updateChanged.listen((e) => useUpdates.getState().receive(e.payload)),
  ]);
  const [settings, info] = await Promise.all([commands.getSettings(), commands.appInfo()]);
  useHub.setState({ settings, info });
  applyMica(info.mica);
  void useHistory.getState().reload();
  void useModels.getState().refresh();
  void useProviders.getState().refresh();
  void useAi.getState().refresh();
  void useStt.getState().refresh();
  void useUpdates.getState().refresh();
}
