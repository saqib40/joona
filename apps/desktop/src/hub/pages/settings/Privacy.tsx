import { save } from "@tauri-apps/plugin-dialog";
import { FileDown, FolderOpen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Group, Row } from "@/components/ui/layout";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { commands, type ExportFormat } from "@/lib/bindings";
import { plural } from "../../format";
import { saveSettings } from "../../settings";
import { type Settings, useHub } from "../../store";

const RETENTION = [
  { value: "0", label: "Keep everything" },
  { value: "7", label: "After a week" },
  { value: "30", label: "After 30 days" },
  { value: "90", label: "After 90 days" },
  { value: "365", label: "After a year" },
];

export function Privacy({ settings }: { settings: Settings }) {
  const { privacy } = settings;
  const days = String(privacy.historyDays);
  return (
    <>
      <Group
        title="History"
        description="Dictations are saved only on this PC, so you can search them and see your Insights."
      >
        <Row title="Keep history">
          <Switch
            aria-label="Keep history"
            checked={privacy.keepHistory}
            onCheckedChange={(on) => void saveSettings((s) => void (s.privacy.keepHistory = on))}
          />
        </Row>
        <Row title="Delete old dictations">
          <Select
            label="Delete old dictations"
            value={days}
            disabled={!privacy.keepHistory}
            options={
              RETENTION.some((o) => o.value === days)
                ? RETENTION
                : [...RETENTION, { value: days, label: `After ${days} days` }]
            }
            onChange={(v) => void saveSettings((s) => void (s.privacy.historyDays = Number(v)))}
          />
        </Row>
        <Row
          title="Save recordings"
          description="Keeps the audio of each dictation, so you can try it again with another model. About 2 MB a minute."
        >
          <Switch
            aria-label="Save recordings"
            checked={privacy.saveAudio}
            disabled={!privacy.keepHistory}
            onCheckedChange={(on) => void saveSettings((s) => void (s.privacy.saveAudio = on))}
          />
        </Row>
      </Group>

      <Group
        title="Cloud services"
        description="Audio only leaves this PC for languages you send to Sarvam, or providers you add yourself."
      >
        <Row title="Fall back to this PC" description="When a cloud service fails or you're offline, transcribe locally.">
          <Switch
            aria-label="Fall back to this PC"
            checked={privacy.fallbackToLocal}
            onCheckedChange={(on) => void saveSettings((s) => void (s.privacy.fallbackToLocal = on))}
          />
        </Row>
        <Row title="Tell me when that happens">
          <Switch
            aria-label="Tell me when that happens"
            checked={privacy.fallbackToast}
            disabled={!privacy.fallbackToLocal}
            onCheckedChange={(on) => void saveSettings((s) => void (s.privacy.fallbackToast = on))}
          />
        </Row>
      </Group>

      <YourData />
    </>
  );
}

function YourData() {
  const dataDir = useHub((s) => s.info?.dataDir);
  const exportAs = async (format: ExportFormat) => {
    const stamp = new Date().toISOString().slice(0, 10);
    const path = await save({
      title: "Export history",
      defaultPath: `joona-history-${stamp}.${format}`,
      filters: [format === "csv" ? { name: "Spreadsheet", extensions: ["csv"] } : { name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    try {
      const count = await commands.historyExport(path, format);
      toast.success(`Exported ${plural(count, "dictation")}`);
    } catch (e) {
      toast.error("Couldn't export", { description: String(e) });
    }
  };
  const clear = async () => {
    try {
      await commands.historyClear();
      toast.success("History deleted");
    } catch (e) {
      toast.error("Couldn't delete the history", { description: String(e) });
    }
  };

  return (
    <Group title="Your data">
      <Row title="Export history" description="Every dictation with its date, app and language.">
        <Button onClick={() => void exportAs("csv")}>
          <FileDown />
          CSV
        </Button>
        <Button onClick={() => void exportAs("json")}>
          <FileDown />
          JSON
        </Button>
      </Row>
      <Row title="Data folder" description={dataDir}>
        <Button onClick={() => void commands.openFolder("data")}>
          <FolderOpen />
          Open
        </Button>
      </Row>
      <Row title="Delete all history" description="Removes every dictation and recording from this PC. Settings and models stay.">
        <Confirm
          title="Delete all history?"
          description="Every dictation and saved recording is removed from this PC. This can't be undone."
          confirm="Delete everything"
          onConfirm={() => void clear()}
        >
          <Button variant="danger">Delete all…</Button>
        </Confirm>
      </Row>
    </Group>
  );
}
