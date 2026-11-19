import { Check, ChevronDown, Download, RotateCcw, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, Dots, ProgressBar } from "@/components/ui/layout";
import { cn } from "@/lib/utils";
import { formatBytes } from "../format";
import { downloadState, fraction, type ModelCard, useModels } from "../models";
import { Footer, type StepProps, StepLayout } from "./layout";

export function ModelStep({ next, back }: StepProps) {
  const overview = useModels((s) => s.overview);
  const downloads = useModels((s) => s.downloads);
  const error = useModels((s) => s.error);
  const [more, setMore] = useState(false);
  useEffect(() => void useModels.getState().refresh(), []);

  const ready = overview?.models.find((m) => m.id === overview.current && m.installed);
  const recommended = overview?.models.find((m) => m.id === overview.recommended);
  const downloading = Object.values(downloads).some((p) => downloadState(p).kind === "running");
  const choices = (overview?.models ?? []).filter(
    (m) => m.fit !== "tooBig" && m.id !== (ready ?? recommended)?.id,
  );

  return (
    <StepLayout
      title="Get your speech model"
      description="Speech turns into text right here on this PC, so your voice never leaves it. Download once, and it works offline."
      footer={
        <Footer
          back={back}
          next={next}
          label={ready ? "Continue" : downloading ? "Continue while it downloads" : "Skip for now"}
        />
      }
    >
      {!overview && <p className="text-[13px] text-ink-faint">{error ?? "Looking at this PC…"}</p>}
      {ready ? <Ready model={ready} /> : recommended && <Hero model={recommended} />}
      {choices.length > 0 && (
        <div className="mt-5">
          <button
            type="button"
            aria-expanded={more}
            onClick={() => setMore((m) => !m)}
            className="flex items-center gap-1.5 text-[13px] font-medium text-ink-soft transition-colors hover:text-ink"
          >
            <ChevronDown className={cn("size-4 transition-transform", more && "rotate-180")} />
            {more ? "Fewer choices" : "Other models"}
          </button>
          {more && (
            <Card className="mt-3 divide-y divide-line overflow-hidden animate-fade-in">
              {choices.map((m) => (
                <Line key={m.id} model={m} current={overview?.current === m.id} />
              ))}
            </Card>
          )}
        </div>
      )}
    </StepLayout>
  );
}

function Ready({ model }: { model: ModelCard }) {
  return (
    <Card className="flex items-center gap-4 p-5">
      <span className="grid size-11 shrink-0 place-items-center rounded-full bg-success-soft text-success">
        <Check className="size-5" strokeWidth={2.5} />
      </span>
      <div className="min-w-0">
        <p className="text-[15px] font-semibold text-ink">{model.name} is ready</p>
        <p className="mt-0.5 text-[13px] text-ink-soft">{model.summary}</p>
      </div>
    </Card>
  );
}

function Hero({ model }: { model: ModelCard }) {
  const progress = useModels((s) => s.downloads[model.id]);
  const state = downloadState(progress);
  const share = state.kind === "idle" ? null : fraction(state.progress);
  return (
    <Card className="p-6">
      <div className="flex items-start justify-between gap-6">
        <div className="min-w-0">
          <Badge tone="accent">Best for you</Badge>
          <h2 className="mt-3 font-display text-[30px] leading-none text-ink">{model.name}</h2>
          <p className="mt-2 text-[13.5px] leading-relaxed text-ink-soft">{model.summary}</p>
          <div className="mt-3.5 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[12.5px] text-ink-soft">
            <Dots value={model.speed} label="Speed" />
            <Dots value={model.accuracy} label="Accuracy" />
            <span>{formatBytes(model.downloadSize)}</span>
          </div>
        </div>
        <div className="shrink-0 pt-1">
          {state.kind === "running" ? (
            <Button size="lg" onClick={() => void useModels.getState().cancel(model.id)}>
              <X />
              Pause
            </Button>
          ) : (
            <Button variant="primary" size="lg" onClick={() => void useModels.getState().download(model.id)}>
              {state.kind === "failed" ? <RotateCcw /> : <Download />}
              {state.kind === "failed" ? "Try again" : state.kind === "paused" ? "Resume" : "Download"}
            </Button>
          )}
        </div>
      </div>
      {state.kind !== "idle" && (
        <div className="mt-5 space-y-1.5">
          {state.kind !== "failed" && <ProgressBar value={share} />}
          <p className={cn("text-[12.5px] text-ink-soft tabular", state.kind === "failed" && "text-danger")}>
            {state.kind === "failed"
              ? (state.progress.error ?? "The download failed.")
              : state.kind === "paused"
                ? `Paused at ${formatBytes(state.progress.done)} of ${formatBytes(state.progress.total)}.`
                : state.progress.stage === "downloading"
                  ? `${formatBytes(state.progress.done)} of ${formatBytes(state.progress.total)}${state.progress.bytesPerSec > 0 ? ` · ${formatBytes(state.progress.bytesPerSec)}/s` : ""}`
                  : "Almost done…"}
          </p>
        </div>
      )}
    </Card>
  );
}

function Line({ model, current }: { model: ModelCard; current: boolean }) {
  const progress = useModels((s) => s.downloads[model.id]);
  const state = downloadState(progress);
  const share = state.kind === "running" ? fraction(state.progress) : null;
  return (
    <div className="flex items-center gap-4 px-4 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-[13.5px] font-medium text-ink">
          {model.name}
          {model.fit === "slow" && <span className="ml-2 text-[12px] font-normal text-accent-ink">Slow on this PC</span>}
          {model.fit === "wrongLanguages" && (
            <span className="ml-2 text-[12px] font-normal text-accent-ink">Not all your languages</span>
          )}
        </p>
        <p className="truncate text-[12.5px] text-ink-soft">{model.summary}</p>
      </div>
      <span className="shrink-0 text-[12px] text-ink-faint tabular">
        {state.kind === "running" && share !== null ? `${Math.floor(share * 100)}%` : formatBytes(model.downloadSize)}
      </span>
      {model.installed ? (
        current ? (
          <Badge tone="ink">In use</Badge>
        ) : (
          <Button size="sm" onClick={() => void useModels.getState().use(model.id, false)}>
            Use
          </Button>
        )
      ) : state.kind === "running" ? (
        <Button size="sm" onClick={() => void useModels.getState().cancel(model.id)}>
          Pause
        </Button>
      ) : (
        <Button size="sm" onClick={() => void useModels.getState().download(model.id)}>
          {state.kind === "paused" ? "Resume" : state.kind === "failed" ? "Retry" : "Download"}
        </Button>
      )}
    </div>
  );
}
