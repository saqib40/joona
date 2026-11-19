import { openUrl } from "@tauri-apps/plugin-opener";
import { Bug, Download, ExternalLink, FileText, FolderOpen, LoaderCircle, RefreshCw } from "lucide-react";
import { useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Card, Group, ProgressBar, Row } from "@/components/ui/layout";
import { Switch } from "@/components/ui/switch";
import { commands, type Folder } from "@/lib/bindings";
import { useModels } from "../../models";
import { saveSettings } from "../../settings";
import { type Settings, useHub } from "../../store";
import { useUpdates } from "../../updates";

const REPO = "https://github.com/saqib40/joona";

const LIBRARIES: { name: string; license: string; url: string }[] = [
  { name: "whisper.cpp", license: "MIT", url: "https://github.com/ggml-org/whisper.cpp" },
  { name: "ONNX Runtime", license: "MIT", url: "https://github.com/microsoft/onnxruntime" },
  { name: "transcribe-rs", license: "MIT", url: "https://github.com/cjpais/transcribe-rs" },
  { name: "Tauri", license: "MIT or Apache-2.0", url: "https://tauri.app" },
  { name: "React", license: "MIT", url: "https://react.dev" },
  { name: "Radix UI", license: "MIT", url: "https://www.radix-ui.com" },
  { name: "Lucide icons", license: "ISC", url: "https://lucide.dev" },
  { name: "Inter and Instrument Serif", license: "SIL Open Font License", url: "https://fonts.google.com" },
];

const open = (url: string) => void openUrl(url);
const openFolder = (folder: Folder) => void commands.openFolder(folder);

export function About({ settings }: { settings: Settings }) {
  const info = useHub((s) => s.info);
  const models = useModels((s) => s.overview?.models);
  const credited = (models ?? []).filter((m) => m.credit && !m.custom);

  return (
    <>
      <Card className="flex items-center gap-5 p-6">
        <span className="grid size-14 shrink-0 place-items-center rounded-2xl bg-primary shadow-soft">
          <span className="flex items-end gap-[3px]">
            {[11, 19, 13].map((h, i) => (
              <span key={i} className="w-[5px] rounded-full bg-accent" style={{ height: h }} />
            ))}
          </span>
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="font-display text-[32px] leading-none text-ink">Joona</h2>
          <p className="mt-1.5 text-[13px] text-ink-soft">
            Version {info?.version ?? "…"} · {info?.os ?? "Windows"}
          </p>
          <p className="mt-0.5 text-[12.5px] text-ink-faint">Free and open source under the MIT licence.</p>
        </div>
        <div className="flex shrink-0 flex-col gap-2">
          <Button onClick={() => open(REPO)}>
            <ExternalLink />
            Source code
          </Button>
          <Button variant="ghost" onClick={() => open(`${REPO}/issues/new`)}>
            <Bug />
            Report a problem
          </Button>
        </div>
      </Card>

      <Updates settings={settings} version={info?.version ?? null} />

      <Group title="Troubleshooting">
        <Row title="Logs" description="Attach these when you report a problem. They never include what you said.">
          <Button onClick={() => openFolder("logs")}>
            <FolderOpen />
            Open
          </Button>
        </Row>
        <Row title="Settings file" description={info?.settingsFile}>
          <Button onClick={() => openFolder("data")}>
            <FolderOpen />
            Open folder
          </Button>
        </Row>
      </Group>

      {credited.length > 0 && (
        <Group title="Models" description="Speech models by these teams, used under their licences.">
          {credited.map((model) => (
            <Row key={model.id} title={model.name} description={model.credit}>
              <span className="text-[12px] text-ink-faint">{model.license}</span>
            </Row>
          ))}
        </Group>
      )}

      <Group
        title="Built with"
        action={
          <Button size="sm" variant="ghost" onClick={() => void commands.openLicenses()}>
            <FileText />
            All licences
          </Button>
        }
      >
        {LIBRARIES.map((library) => (
          <Row
            key={library.name}
            title={
              <button
                type="button"
                onClick={() => open(library.url)}
                className="inline-flex items-center gap-1 hover:underline"
              >
                {library.name}
                <ExternalLink className="size-3 text-ink-faint" />
              </button>
            }
          >
            <span className="text-[12px] text-ink-faint">{library.license}</span>
          </Row>
        ))}
      </Group>
    </>
  );
}

function Updates({ settings, version }: { settings: Settings; version: string | null }) {
  const status = useUpdates((s) => s.status);
  useEffect(() => void useUpdates.getState().refresh(), []);
  const { check, install } = useUpdates.getState();
  const current = version ? `Joona ${version}` : "Joona";

  const checkNow = (
    <Button onClick={() => void check()}>
      <RefreshCw />
      Check now
    </Button>
  );
  const row = (() => {
    switch (status?.state) {
      case undefined:
      case "unsupported":
        return <Row title={current} description="Development builds don't update themselves." />;
      case "unknown":
        return (
          <Row title={current} description="Joona hasn't looked for a newer version yet.">
            {checkNow}
          </Row>
        );
      case "upToDate":
        return (
          <Row title={current} description="You have the latest version.">
            {checkNow}
          </Row>
        );
      case "checking":
        return (
          <Row title={current} description="Looking for a newer version…">
            <Button disabled>
              <LoaderCircle className="animate-spin" />
              Checking
            </Button>
          </Row>
        );
      case "available":
        return (
          <Row
            title={`Joona ${status.version} is available`}
            description={firstLine(status.notes) ?? "Joona closes for a few seconds while it updates, then opens again."}
          >
            <Button onClick={() => void install()}>
              <Download />
              Update and restart
            </Button>
          </Row>
        );
      case "downloading":
        return (
          <Row
            title={`Downloading Joona ${status.version}`}
            description={
              <ProgressBar value={status.percent === null ? null : status.percent / 100} className="mt-2 w-72" />
            }
          >
            {status.percent !== null && <span className="text-[12px] text-ink-faint tabular">{status.percent}%</span>}
          </Row>
        );
      case "installing":
        return (
          <Row
            title={`Installing Joona ${status.version}`}
            description="Joona closes now, and opens again when the update is in."
          />
        );
      case "failed":
        return (
          <Row title="Couldn't update" description={status.message}>
            <Button onClick={() => void check()}>
              <RefreshCw />
              Try again
            </Button>
          </Row>
        );
    }
  })();

  return (
    <Group title="Updates">
      {row}
      <Row
        title="Check for updates automatically"
        description="Twice a day, Joona asks GitHub whether there's a new version. Nothing you dictate is sent."
      >
        <Switch
          aria-label="Check for updates automatically"
          checked={settings.general.checkUpdates}
          onCheckedChange={(on) => void saveSettings((s) => void (s.general.checkUpdates = on))}
        />
      </Row>
    </Group>
  );
}

/** Release notes can be long; the first line says what changed. */
function firstLine(notes: string | null): string | null {
  const line = notes
    ?.split("\n")
    .map((l) => l.replace(/^[#>*\-\s]+/, "").trim())
    .find((l) => l.length > 0);
  return line ?? null;
}
