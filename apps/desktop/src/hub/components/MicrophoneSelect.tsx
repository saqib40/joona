import { Select } from "@/components/ui/select";
import { commands } from "@/lib/bindings";
import { isMac } from "@/lib/utils";
import { useLoad } from "../hooks";
import { saveSettings } from "../settings";

/** Radix Select can't hold an empty value. */
const DEFAULT_MIC = "default";

/** Picks the microphone, or the Windows default. `missing` is a saved one that isn't connected. */
export function useMicrophones(device: string | null, reload: unknown = 0) {
  const { data: devices } = useLoad(() => commands.listMicrophones(), [reload]);
  const missing = device !== null && devices !== null && !devices.some((d) => d.id === device);
  return { devices, missing };
}

export function MicrophoneSelect({ device, reload, className }: { device: string | null; reload?: unknown; className?: string }) {
  const { devices, missing } = useMicrophones(device, reload);
  const fallback = devices?.find((d) => d.isDefault)?.name;
  const options = [
    { value: DEFAULT_MIC, label: isMac ? "System default" : "Windows default", hint: fallback },
    ...(devices ?? []).map((d) => ({ value: d.id, label: d.name })),
    ...(missing && device ? [{ value: device, label: "Not connected" }] : []),
  ];
  return (
    <Select
      label="Microphone"
      className={className ?? "max-w-72"}
      value={device ?? DEFAULT_MIC}
      options={options}
      onChange={(id) => void saveSettings((s) => void (s.general.microphone = id === DEFAULT_MIC ? null : id))}
    />
  );
}
