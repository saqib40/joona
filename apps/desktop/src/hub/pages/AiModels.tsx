import { Check, Cloud, Download, HardDrive, MemoryStick, RotateCcw, Sparkles, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Card, Group, Row } from "@/components/ui/layout";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import {
  type AiOverview,
  type AiProvider,
  type AiSample,
  choiceKey,
  LOCAL_AI,
  LOCAL_AI_DOWNLOAD,
  type LocalAiInfo,
  modelLabel,
  useAi,
} from "../ai";
import { AiSheet } from "../components/AiSheet";
import { DownloadProgress } from "../components/DownloadProgress";
import { formatBytes, formatMemory } from "../format";
import { downloadState, useModels } from "../models";
import { SARVAM, useProviders, useSarvam } from "../providers";
import { saveSettings } from "../settings";
import type { Settings } from "../store";
import { CommandModeRow } from "./settings/Experimental";

/** Servers people run themselves, listed apart from the cloud services. */
const OWN_SERVERS = ["ollama", "lmstudio", "custom"];

/** Smart polish's AI model: the one in use, Joona's own, and the providers to choose from. */
export function AiModels({ settings }: { settings: Settings }) {
  const overview = useAi((s) => s.overview);
  const keys = useProviders((s) => s.keys);
  const [sheet, setSheet] = useState<AiProvider | null>(null);
  const { polish, privacy } = settings;
  // Whether the model is ready depends on these.
  useEffect(() => void useAi.getState().refresh(), [polish, privacy.consented, keys]);
  if (!overview) return null;

  const open = (id: string) => setSheet(overview.providers.find((p) => p.id === id) ?? null);
  const cloud = overview.providers.filter((p) => p.id !== LOCAL_AI && !OWN_SERVERS.includes(p.id));
  const own = overview.providers.filter((p) => OWN_SERVERS.includes(p.id));
  return (
    <div className="mt-8 space-y-6">
      <InUse settings={settings} overview={overview} onChange={open} />
      {overview.local && <LocalAi info={overview.local} inUse={polish.provider === LOCAL_AI} />}
      <ProviderList
        title="With your API key"
        description="You pay the company for what you use. A dictation usually costs a fraction of a cent."
        providers={cloud}
        current={polish.provider}
        onOpen={open}
      />
      <ProviderList
        title="On a server you run"
        description="Ollama, LM Studio or any server with an OpenAI-compatible API."
        providers={own}
        current={polish.provider}
        onOpen={open}
      />
      <AiSheet provider={sheet} onOpenChange={(isOpen) => !isOpen && setSheet(null)} />
    </div>
  );
}

function InUse({
  settings,
  overview,
  onChange,
}: {
  settings: Settings;
  overview: AiOverview;
  onChange: (id: string) => void;
}) {
  const sample = useAi((s) => s.sample);
  const testing = useAi((s) => s.testing);
  const sarvam = useSarvam();
  const { ready, problem } = overview;
  const picked = overview.providers.find((p) => p.id === settings.polish.provider);
  const smart = settings.polish.level === "smart";
  const name = ready?.name ?? picked?.name ?? "";

  const test = async () => {
    const failure = await useAi.getState().test();
    if (failure) toast.error(`${name} didn't work`, { description: failure.message });
  };

  return (
    <Group
      title="AI models"
      description="For Smart polish and translating text. A model gets the text of your dictations, never your recordings."
      action={
        ready ? (
          ready.cloud ? (
            <Badge tone="cloud">
              <Cloud />
              Cloud
            </Badge>
          ) : (
            <Badge tone="success">On this PC</Badge>
          )
        ) : picked ? (
          <Badge tone="danger">Not ready</Badge>
        ) : (
          <Badge>Off</Badge>
        )
      }
    >
      {ready ? (
        <div className="px-4 py-3.5">
          <div className="flex items-center gap-3.5">
            <span className="grid size-9 shrink-0 place-items-center rounded-full bg-accent-soft text-accent-ink">
              <Sparkles className="size-4" strokeWidth={2} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-[13.5px] font-medium text-ink">{ready.name}</p>
              <p className="mt-0.5 truncate text-[12.5px] text-ink-soft">{modelLabel(ready, overview)}</p>
            </div>
            <Button disabled={testing} onClick={() => void test()}>
              {testing ? "Trying…" : "Try it"}
            </Button>
            {ready.provider !== LOCAL_AI && (
              <Button variant="ghost" onClick={() => onChange(ready.provider)}>
                Change
              </Button>
            )}
          </div>
          {sample?.by === choiceKey(ready) && <Sample sample={sample.result} />}
        </div>
      ) : picked ? (
        <Row title={`${picked.name} isn't ready`} description={`${problem ?? ""} Until it is, Joona uses Basic polish.`}>
          {picked.id !== LOCAL_AI && (
            <Button variant="primary" onClick={() => onChange(picked.id)}>
              Fix
            </Button>
          )}
        </Row>
      ) : (
        <Row
          title="No AI model yet"
          description="Joona's own model below is free and keeps your text on this PC. Or use your own key for OpenAI, Anthropic, Groq and others."
        />
      )}
      {picked && (
        <>
          <Row
            title="Smart polish"
            description="Fixes grammar, follows corrections like “no, Tuesday”, and turns spoken lists into lists."
          >
            <Switch
              aria-label="Smart polish"
              checked={smart}
              onCheckedChange={(on) => void saveSettings((s) => void (s.polish.level = on ? "smart" : "basic"))}
            />
          </Row>
          <Row
            title="Translate text"
            description={`When your languages need text translated or written in English letters.${
              sarvam.ready ? (smart ? ` ${name} does it while polishing, instead of Sarvam.` : " Sarvam does it when it can.") : ""
            }`}
          >
            <Switch
              aria-label="Translate text"
              checked={settings.polish.translate}
              onCheckedChange={(on) => void saveSettings((s) => void (s.polish.translate = on))}
            />
          </Row>
          <CommandModeRow settings={settings} />
          <Row title={`Stop using ${name}`} description="Smart polish goes back to Basic. Saved keys stay saved.">
            <Button variant="quiet" onClick={() => void useAi.getState().stop()}>
              Stop using
            </Button>
          </Row>
        </>
      )}
    </Group>
  );
}

function Sample({ sample }: { sample: AiSample }) {
  return (
    <dl className="mt-3.5 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-lg bg-fill px-3.5 py-3 text-[12.5px] leading-relaxed">
      <dt className="text-ink-faint">You said</dt>
      <dd className="text-ink-soft">{sample.before}</dd>
      <dt className="text-ink-faint">Joona wrote</dt>
      <dd className="text-ink">
        {sample.after}
        <span className="ml-2 text-[11.5px] text-ink-faint tabular">{(sample.ms / 1000).toFixed(1)} s</span>
      </dd>
    </dl>
  );
}

function LocalAi({ info, inUse }: { info: LocalAiInfo; inUse: boolean }) {
  const live = useModels((s) => s.downloads[LOCAL_AI_DOWNLOAD]);
  const state = downloadState(live ?? info.download ?? undefined);

  const use = async () => {
    if (!(await useAi.getState().use(LOCAL_AI))) return;
    const failure = await useAi.getState().test();
    if (failure) toast.error("Local AI didn't work", { description: failure.message });
  };

  return (
    <Card className={cn("px-5 py-4", inUse && "ring-2 ring-primary/80")}>
      <div className="flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-[15px] font-semibold text-ink">Local AI</h3>
            <Badge tone="success">Free</Badge>
            {inUse && (
              <Badge tone="ink">
                <Check strokeWidth={3} />
                In use
              </Badge>
            )}
          </div>
          <p className="mt-1 text-[13px] leading-snug text-ink-soft">
            {info.name} runs on this PC, so your text never leaves it. No account or key needed.
          </p>
          <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[12px] text-ink-soft">
            <span className="flex items-center gap-1">
              <HardDrive className="size-3 text-ink-faint" />
              {formatBytes(info.size)}
            </span>
            <span className="flex items-center gap-1">
              <MemoryStick className="size-3 text-ink-faint" />
              {formatMemory(info.ramMb)} memory while it works
            </span>
            {info.running && <span>Loaded</span>}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
          {info.installed ? (
            <>
              {!inUse && (
                <Button variant="primary" size="sm" onClick={() => void use()}>
                  Use
                </Button>
              )}
              <Confirm
                title="Delete the local AI model?"
                description={`This frees ${formatBytes(info.size)}.${
                  inUse ? " Smart polish goes back to Basic until you pick another model." : ""
                } You can download it again at any time.`}
                confirm="Delete"
                onConfirm={() => void useAi.getState().remove()}
              >
                <Button variant="ghost" size="sm">
                  Delete…
                </Button>
              </Confirm>
            </>
          ) : state.kind === "running" ? (
            <Button size="sm" onClick={() => void useAi.getState().cancel()}>
              <X />
              Cancel
            </Button>
          ) : (
            <Button size="sm" variant="primary" onClick={() => void useAi.getState().download()}>
              {state.kind === "failed" ? <RotateCcw /> : <Download />}
              {state.kind === "failed" ? "Retry" : state.kind === "paused" ? "Resume" : "Download"}
            </Button>
          )}
        </div>
      </div>
      {!info.installed && state.kind !== "idle" && <DownloadProgress progress={state.progress} />}
      <p className="mt-3 text-[11.5px] leading-relaxed text-ink-faint">
        {info.credit}. {info.license} licence.
      </p>
    </Card>
  );
}

function ProviderList({
  title,
  description,
  providers,
  current,
  onOpen,
}: {
  title: string;
  description: string;
  providers: AiProvider[];
  current: string | null;
  onOpen: (id: string) => void;
}) {
  const keys = useProviders((s) => s.keys);
  return (
    <Group title={title} description={description}>
      {providers.map((p) => {
        const keySaved = p.key !== "never" && (keys?.includes(p.id) ?? false);
        return (
          <Row key={p.id} title={p.name} description={keySaved && p.id !== SARVAM ? `${p.summary} Key saved.` : p.summary}>
            {keySaved && p.id !== SARVAM && (
              <Confirm
                title={`Delete your ${p.name} key?`}
                description={`Joona deletes the key from this PC${current === p.id ? ` and stops using ${p.name}` : ""}.`}
                confirm="Delete"
                onConfirm={() => void useProviders.getState().remove(p.id)}
              >
                <Button variant="ghost" size="icon-sm" aria-label={`Delete your ${p.name} key`}>
                  <Trash2 />
                </Button>
              </Confirm>
            )}
            {current === p.id ? (
              <Badge tone="ink">
                <Check strokeWidth={3} />
                In use
              </Badge>
            ) : (
              <Button onClick={() => onOpen(p.id)}>{p.key === "required" && !keySaved ? "Add key…" : "Use…"}</Button>
            )}
          </Row>
        );
      })}
    </Group>
  );
}
