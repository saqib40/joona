import { Download, Flame, Gauge, Pause, Type } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Keys } from "@/components/ui/kbd";
import { Card, PageHeader } from "@/components/ui/layout";
import { commands, type HomeStats } from "@/lib/bindings";
import { formatBytes, formatNumber, greeting } from "../format";
import { useHistory } from "../history";
import { useLoad } from "../hooks";
import { useModels } from "../models";
import { shortcutKeys } from "../shortcut";
import { useHub } from "../store";
import { HistoryList } from "./home/HistoryList";

export function Home() {
  const info = useHub((s) => s.info);
  const settings = useHub((s) => s.settings);
  const version = useHistory((s) => s.version);
  const { data: stats } = useLoad(() => commands.homeStats(), [version]);
  const trigger = settings ? shortcutKeys(settings.shortcuts.trigger) : ["Ctrl", "Win"];
  const doubleTap = settings?.shortcuts.doubleTapForHandsFree ?? true;

  return (
    <>
      <PageHeader
        title={greeting(new Date(), info?.firstName)}
        description={
          <>
            Hold <Keys keys={trigger} className="mx-0.5" /> in any app and talk. Let go and your words are typed.
            {doubleTap && " Double-tap for hands-free."}
          </>
        }
      />
      <StatStrip stats={stats} />
      <Callout />
      <HistoryList />
    </>
  );
}

function StatStrip({ stats }: { stats: HomeStats | null }) {
  const items = [
    { icon: Flame, value: stats ? formatNumber(stats.streak) : "–", label: "day streak" },
    { icon: Type, value: stats ? formatNumber(stats.wordsThisWeek) : "–", label: "words this week" },
    { icon: Gauge, value: stats?.wpm ? formatNumber(stats.wpm) : "–", label: "words per minute" },
  ];
  return (
    <div className="grid grid-cols-3 gap-3">
      {items.map(({ icon: Icon, value, label }) => (
        <Card key={label} className="flex items-center gap-3.5 px-4 py-3.5">
          <span className="grid size-9 shrink-0 place-items-center rounded-full bg-accent-soft text-accent-ink">
            <Icon className="size-[17px]" strokeWidth={2} />
          </span>
          <span className="min-w-0">
            <span className="block text-[21px] font-semibold leading-tight tracking-[-0.01em] text-ink tabular">
              {value}
            </span>
            <span className="block truncate text-[12.5px] text-ink-soft">{label}</span>
          </span>
        </Card>
      ))}
    </div>
  );
}

/** Says what's in the way of dictating, if anything. */
function Callout() {
  const settings = useHub((s) => s.settings);
  const overview = useModels((s) => s.overview);
  const downloads = useModels((s) => s.downloads);

  if (settings?.general.paused) {
    return (
      <Card className="mt-4 flex items-center gap-4 px-5 py-4">
        <Pause className="size-5 shrink-0 text-ink-soft" />
        <div className="min-w-0 flex-1">
          <p className="text-[13.5px] font-medium text-ink">Joona is paused</p>
          <p className="text-[12.5px] text-ink-soft">The shortcut does nothing until you resume.</p>
        </div>
        <Button
          variant="primary"
          onClick={() => void commands.setPaused(false).catch((e) => toast.error(String(e)))}
        >
          Resume
        </Button>
      </Card>
    );
  }

  if (!overview || overview.models.some((m) => m.installed)) return null;
  const pick = overview.models.find((m) => m.id === overview.recommended) ?? overview.models[0];
  if (!pick) return null;
  const running = Object.values(downloads).some((p) => !["done", "failed", "cancelled"].includes(p.stage));
  return (
    <Card className="mt-4 overflow-hidden">
      <div className="flex items-center gap-4 bg-accent-soft/60 px-5 py-4">
        <span className="grid size-10 shrink-0 place-items-center rounded-full bg-accent text-[oklch(0.22_0.03_60)]">
          <Download className="size-[18px]" strokeWidth={2.2} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[14px] font-semibold text-ink">
            {running ? "Your speech model is on its way" : "One download and you're ready"}
          </p>
          <p className="text-[12.5px] leading-snug text-ink-soft">
            Joona turns speech into text on this PC. {pick.name} suits your languages and this PC (
            {formatBytes(pick.downloadSize)}).
          </p>
        </div>
        {running ? (
          <Button onClick={() => useHub.getState().navigate("models")}>See progress</Button>
        ) : (
          <div className="flex gap-2">
            <Button variant="ghost" onClick={() => useHub.getState().navigate("models")}>
              All models
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                void useModels.getState().download(pick.id);
                useHub.getState().navigate("models");
              }}
            >
              Get {pick.name}
            </Button>
          </div>
        )}
      </div>
    </Card>
  );
}
