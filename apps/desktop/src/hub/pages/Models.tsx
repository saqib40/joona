import {
  Check,
  Cpu,
  Download,
  Ellipsis,
  FolderOpen,
  HardDrive,
  MemoryStick,
  RotateCcw,
  TriangleExclamationPoint,
  X,
  Zap,
} from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Card, Dots, Group, PageHeader, Row } from "@/components/ui/layout";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { commands } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { DownloadProgress } from "../components/DownloadProgress";
import { formatBytes, formatMemory } from "../format";
import { languageName } from "../languages";
import { downloadState, type ModelCard, type ModelsOverview, useModels } from "../models";
import { saveSettings } from "../settings";
import { useHub } from "../store";

const IDLE_OPTIONS = [
  { value: "5", label: "After 5 minutes" },
  { value: "10", label: "After 10 minutes" },
  { value: "30", label: "After 30 minutes" },
  { value: "60", label: "After an hour" },
  { value: "0", label: "Never" },
];

export function Models() {
  const overview = useModels((s) => s.overview);
  const error = useModels((s) => s.error);
  const openFolder = () =>
    void commands.openModelsFolder().catch((e) => toast.error("Couldn't open the folder", { description: String(e) }));

  return (
    <>
      <PageHeader
        title="Models"
        description="Speech recognition runs on this PC. A model downloads once and then works offline."
        actions={
          <Button onClick={openFolder}>
            <FolderOpen />
            Models folder
          </Button>
        }
      />
      {error && !overview && <p className="text-[13px] text-danger">{error}</p>}
      {overview && <Overview overview={overview} />}
    </>
  );
}

function Overview({ overview }: { overview: ModelsOverview }) {
  const settings = useHub((s) => s.settings);
  const installed = overview.models.filter((m) => m.installed);
  const available = overview.models
    .filter((m) => !m.installed)
    .sort((a, b) => Number(b.id === overview.recommended) - Number(a.id === overview.recommended));
  const autoMode = !settings?.models.active;

  return (
    <div className="space-y-8">
      <ThisPc overview={overview} />
      {installed.length > 0 && (
        <section className="space-y-2.5">
          <SectionTitle
            title="On this PC"
            hint={autoMode ? "Joona chooses automatically based on your hardware" : "Manual choice active"}
          />
          {installed.map((m) => (
            <ModelRow key={m.id} model={m} overview={overview} />
          ))}
        </section>
      )}
      {available.length > 0 && (
        <section className="space-y-2.5">
          <SectionTitle
            title={installed.length > 0 ? "More models" : "Choose a model"}
            hint="Bigger models are more accurate and need more memory."
          />
          {available.map((m) => (
            <ModelRow key={m.id} model={m} overview={overview} />
          ))}
        </section>
      )}
    </div>
  );
}

function SectionTitle({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="flex items-baseline justify-between px-1">
      <h2 className="text-[13px] font-semibold text-ink">{title}</h2>
      {hint && <p className="text-[12px] text-ink-faint">{hint}</p>}
    </div>
  );
}

function ThisPc({ overview }: { overview: ModelsOverview }) {
  const models = useHub((s) => s.settings?.models);
  const idle = models?.idleUnloadMinutes ?? 10;
  const { hardware, engine } = overview;
  const gpu = hardware.gpus.find((g) => g.dedicated) ?? hardware.gpus[0];
  const loaded = engine.loaded
    .map((id) => overview.models.find((m) => m.id === id)?.name ?? id)
    .join(", ");
  const status = !engine.running
    ? "The engine starts with your first dictation."
    : loaded
      ? `${loaded} ready on the ${engine.variant === "vulkan" ? "graphics card" : "CPU"}.`
      : `Running on the ${engine.variant === "vulkan" ? "graphics card" : "CPU"}, no model in memory.`;

  return (
    <Group>
      <div className="grid grid-cols-3 divide-x divide-line">
        <Spec icon={Cpu} title="Processor" value={hardware.cpu} detail={`${hardware.cores} cores · ${hardware.threads} threads${hardware.avx2 ? "" : " · no AVX2"}`} />
        <Spec
          icon={MemoryStick}
          title="Memory"
          value={formatMemory(hardware.ramMb)}
          detail={`${formatMemory(hardware.availableRamMb)} free now`}
        />
        <Spec
          icon={Zap}
          title="Graphics"
          value={gpu?.name ?? "None found"}
          detail={gpu ? (gpu.dedicated ? `${formatMemory(gpu.vramMb)} of its own memory` : "Built into the processor") : "Models run on the CPU"}
        />
      </div>
      <Row
        title="Engine"
        description={
          <>
            {status}
            {engine.vad ? " Silence is trimmed before transcribing." : ""}
          </>
        }
      >
        <Badge tone={engine.running ? "success" : "neutral"}>
          <span className={cn("size-1.5 rounded-full", engine.running ? "bg-success" : "bg-ink-faint")} />
          {engine.running ? "Running" : "Idle"}
        </Badge>
      </Row>
      <Row
        title="Use the graphics card"
        description={
          engine.gpuBuild
            ? "Faster on most PCs with a recent graphics card. Joona switches back to the CPU if it fails."
            : "The graphics-card engine isn't installed. It comes with the full installer."
        }
      >
        <Switch
          aria-label="Use the graphics card"
          checked={!!models?.useGpu && engine.gpuBuild}
          disabled={!engine.gpuBuild || !models}
          onCheckedChange={(on) => void saveSettings((s) => void (s.models.useGpu = on))}
        />
      </Row>
      <Row title="Free memory when idle" description="Unloads the model when you haven't dictated for a while.">
        <Select
          label="Free memory when idle"
          value={String(idle)}
          options={
            IDLE_OPTIONS.some((o) => o.value === String(idle))
              ? IDLE_OPTIONS
              : [...IDLE_OPTIONS, { value: String(idle), label: `After ${idle} minutes` }]
          }
          onChange={(v) => void saveSettings((s) => void (s.models.idleUnloadMinutes = Number(v)))}
        />
      </Row>
      <Row title="Get ready at startup" description="Loads the model when Joona starts, so the first dictation is quick.">
        <Switch
          aria-label="Get ready at startup"
          checked={models?.preload ?? true}
          onCheckedChange={(on) => void saveSettings((s) => void (s.models.preload = on))}
        />
      </Row>
    </Group>
  );
}

function Spec({
  icon: Icon,
  title,
  value,
  detail,
}: {
  icon: typeof Cpu;
  title: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="min-w-0 px-4 py-3.5">
      <p className="flex items-center gap-1.5 text-[11.5px] font-medium text-ink-faint">
        <Icon className="size-3.5" />
        {title}
      </p>
      <p className="mt-1 truncate text-[13.5px] font-medium text-ink" title={value}>
        {value}
      </p>
      <p className="mt-0.5 truncate text-[12px] text-ink-soft">{detail}</p>
    </div>
  );
}

function languagesLabel(model: ModelCard): string {
  if (model.languages.length === 0) return model.multilingual ? "Many languages" : "English";
  if (model.languages.length === 1) return languageName(model.languages[0]);
  if (model.languages.length <= 3) return model.languages.map(languageName).join(", ");
  return `${model.languages.length} languages`;
}

const FIT_NOTES: Partial<Record<ModelCard["fit"], string>> = {
  slow: "Runs, but slowly on this PC.",
  tooBig: "Needs more memory than this PC has free.",
  wrongLanguages: "Doesn't understand all the languages you speak.",
};

function ModelRow({ model, overview }: { model: ModelCard; overview: ModelsOverview }) {
  const progress = useModels((s) => s.downloads[model.id]);
  const settings = useHub((s) => s.settings);
  const state = downloadState(progress);
  const inUse = overview.current === model.id;
  const chosen = settings?.models.active === model.id;
  const english = settings?.models.english === model.id;
  const recommended = overview.recommended === model.id;
  const note = FIT_NOTES[model.fit];
  const canEnglish = model.languages.length === 0 || model.languages.includes("en");

  return (
    <Card className={cn("px-5 py-4", inUse && "ring-2 ring-primary/80")}>
      <div className="flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-[15px] font-semibold text-ink">{model.name}</h3>
            {recommended && !model.installed && <Badge tone="accent">Recommended</Badge>}
            {inUse && (
              <Badge tone="ink">
                <Check strokeWidth={3} />
                In use{chosen ? "" : " · Auto"}
              </Badge>
            )}
            {english && (
              <Badge title="Used when dictating in English while another model transcribes other languages">
                For English
              </Badge>
            )}
            {model.custom && <Badge>Added by hand</Badge>}
          </div>
          <p className="mt-1 text-[13px] leading-snug text-ink-soft">{model.summary}</p>
          <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[12px] text-ink-soft">
            {model.speed > 0 && <Dots value={model.speed} label="Speed" />}
            {model.accuracy > 0 && <Dots value={model.accuracy} label="Accuracy" />}
            <span>{languagesLabel(model)}</span>
            {model.translate && <span>Translates to English</span>}
            <span className="flex items-center gap-1">
              <HardDrive className="size-3 text-ink-faint" />
              {formatBytes(model.installed ? model.size : model.downloadSize)}
            </span>
            {model.ramMb > 0 && <span>{formatMemory(model.ramMb)} memory</span>}
          </div>
          {note && (
            <p className="mt-2 flex items-center gap-1.5 text-[12px] text-accent-ink">
              <TriangleExclamationPoint className="size-3.5" />
              {note}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
          {model.installed ? (
            <>
              {!inUse && (
                <Button variant="primary" size="sm" onClick={() => void useModels.getState().use(model.id, false)}>
                  Use
                </Button>
              )}
              {chosen && (
                <Button variant="secondary" size="sm" onClick={() => void useModels.getState().use(null, false)}>
                  Let Joona choose
                </Button>
              )}
              <ModelMenu model={model} chosen={chosen} english={english} canEnglish={canEnglish} />
            </>
          ) : state.kind === "running" ? (
            <Button size="sm" onClick={() => void useModels.getState().cancel(model.id)}>
              <X />
              Cancel
            </Button>
          ) : (
            <Button
              size="sm"
              variant={recommended ? "primary" : "secondary"}
              onClick={() => void useModels.getState().download(model.id)}
            >
              {state.kind === "failed" ? <RotateCcw /> : <Download />}
              {state.kind === "failed" ? "Retry" : state.kind === "paused" ? "Resume" : "Download"}
            </Button>
          )}
        </div>
      </div>
      {!model.installed && state.kind !== "idle" && <DownloadProgress progress={state.progress} />}
    </Card>
  );
}

function ModelMenu({
  model,
  chosen,
  english,
  canEnglish,
}: {
  model: ModelCard;
  chosen: boolean;
  english: boolean;
  canEnglish: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const models = useModels.getState();
  return (
    <>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <Button size="icon-sm" variant="ghost" aria-label={`More for ${model.name}`}>
            <Ellipsis />
          </Button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content
            align="end"
            sideOffset={6}
            className="z-50 min-w-56 rounded-xl bg-surface-raised p-1 text-[13px] text-ink shadow-lifted ring-1 ring-black/[0.07] animate-fade-in dark:ring-white/10"
          >
            {canEnglish && (
              <MenuItem onSelect={() => void models.use(english ? null : model.id, true)}>
                {english ? "Stop using for English" : "Use for English only"}
              </MenuItem>
            )}
            {chosen && <MenuItem onSelect={() => void models.use(null, false)}>Let Joona choose</MenuItem>}
            {(canEnglish || chosen) && <DropdownMenu.Separator className="mx-2 my-1 h-px bg-line" />}
            <MenuItem danger onSelect={() => setConfirming(true)}>
              Delete…
            </MenuItem>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      <Confirm
        open={confirming}
        onOpenChange={setConfirming}
        title={`Delete ${model.name}?`}
        description={`This frees ${formatBytes(model.size)}. You can download it again at any time.`}
        confirm="Delete"
        onConfirm={() => void models.remove(model.id)}
      />
    </>
  );
}

function MenuItem({ onSelect, danger, children }: { onSelect: () => void; danger?: boolean; children: string }) {
  return (
    <DropdownMenu.Item
      onSelect={onSelect}
      className={cn(
        "flex h-8 cursor-default items-center rounded-lg px-2.5 outline-none data-[highlighted]:bg-fill",
        danger && "text-danger data-[highlighted]:bg-danger-soft",
      )}
    >
      {children}
    </DropdownMenu.Item>
  );
}
