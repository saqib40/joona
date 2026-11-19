import { ChartNoAxesColumn, Cloud, Flame, ShieldCheck } from "lucide-react";
import { Card, EmptyState, PageHeader } from "@/components/ui/layout";
import { commands, type DayActivity, type Insights as InsightsData } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { dayKey, formatNumber, formatShortDate, plural } from "../format";
import { useHistory } from "../history";
import { useLoad } from "../hooks";

export function Insights() {
  const version = useHistory((s) => s.version);
  const { data, error } = useLoad(() => commands.insights(), [version]);

  return (
    <>
      <PageHeader title="Insights" description="How you use Joona, worked out on this PC from your history." />
      {error && !data && <p className="text-[13px] text-danger">{error}</p>}
      {data && data.totalDictations === 0 && (
        <Card>
          <EmptyState icon={ChartNoAxesColumn} title="Nothing to show yet">
            Your speed, streaks and favourite apps appear here after a few dictations.
          </EmptyState>
        </Card>
      )}
      {data && data.totalDictations > 0 && <Report data={data} />}
    </>
  );
}

/** Big enough to read at a glance: `45 min`, `3.5 hours`, `23 hours`. */
function timeSaved(minutes: number): { value: string; unit: string } {
  if (minutes < 60) return { value: formatNumber(minutes), unit: "min" };
  const hours = minutes / 60;
  return { value: formatNumber(Number(hours.toFixed(hours < 10 ? 1 : 0))), unit: hours === 1 ? "hour" : "hours" };
}

function Report({ data }: { data: InsightsData }) {
  const saved = timeSaved(data.minutesSaved);
  const metrics = [
    { label: "Words dictated", value: formatNumber(data.totalWords), hint: plural(data.totalDictations, "dictation") },
    { label: "Speaking speed", value: data.wpm ? `${formatNumber(data.wpm)}` : "–", unit: "wpm", hint: "Typing is about 40" },
    { label: "Time saved", value: saved.value, unit: saved.unit, hint: "Compared with typing" },
    { label: "Fixes made", value: formatNumber(data.fixes), hint: "Dictionary and cleanup" },
  ];
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-4 gap-3">
        {metrics.map((m) => (
          <Card key={m.label} className="px-4 py-4">
            <p className="text-[12px] font-medium text-ink-soft">{m.label}</p>
            <p className="mt-1.5 text-[26px] font-semibold leading-none tracking-[-0.02em] text-ink tabular">
              {m.value}
              {m.unit && <span className="ml-1 text-[13px] font-medium tracking-normal text-ink-faint">{m.unit}</span>}
            </p>
            <p className="mt-2 text-[11.5px] text-ink-faint">{m.hint}</p>
          </Card>
        ))}
      </div>

      <Card className="px-5 pb-4 pt-5">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-[13px] font-semibold text-ink">Activity</h2>
          <div className="flex items-center gap-4 text-[12px] text-ink-soft">
            <span className="flex items-center gap-1.5">
              <Flame className="size-3.5 text-accent-ink" />
              {plural(data.currentStreak, "day")} in a row
            </span>
            <span className="text-ink-faint">Longest: {plural(data.longestStreak, "day")}</span>
          </div>
        </div>
        <Heatmap days={data.days} />
      </Card>

      <div className="grid grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)] gap-4">
        <Card className="p-5">
          <h2 className="mb-4 text-[13px] font-semibold text-ink">Where you dictate</h2>
          {data.apps.length === 0 ? (
            <p className="text-[12.5px] text-ink-faint">No apps recorded yet.</p>
          ) : (
            <Apps apps={data.apps} />
          )}
        </Card>
        <Card className="flex flex-col p-5">
          <h2 className="text-[13px] font-semibold text-ink">Where your voice went</h2>
          <div className="mt-4 flex items-center gap-3">
            <span
              className={cn(
                "grid size-10 place-items-center rounded-full",
                data.cloudPercent === 0 ? "bg-success-soft text-success" : "bg-cloud-soft text-cloud",
              )}
            >
              {data.cloudPercent === 0 ? <ShieldCheck className="size-5" /> : <Cloud className="size-5" />}
            </span>
            <div>
              <p className="text-[20px] font-semibold leading-tight text-ink tabular">{100 - data.cloudPercent}%</p>
              <p className="text-[12px] text-ink-soft">transcribed on this PC</p>
            </div>
          </div>
          <p className="mt-auto pt-4 text-[12px] leading-relaxed text-ink-faint">
            {data.cloudPercent === 0
              ? "No audio has left this PC."
              : `${data.cloudPercent}% of dictations used a cloud service you turned on.`}
          </p>
        </Card>
      </div>
    </div>
  );
}

function Apps({ apps }: { apps: InsightsData["apps"] }) {
  const most = Math.max(...apps.map((a) => a.words), 1);
  return (
    <ul className="space-y-3">
      {apps.slice(0, 6).map((app) => (
        <li key={app.app}>
          <div className="flex items-baseline justify-between gap-3 text-[13px]">
            <span className="truncate text-ink">{app.name}</span>
            <span className="shrink-0 text-[12px] text-ink-soft tabular">{formatNumber(app.words)} words</span>
          </div>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-fill-strong">
            <div className="h-full rounded-full bg-ink" style={{ width: `${(app.words / most) * 100}%` }} />
          </div>
        </li>
      ))}
    </ul>
  );
}

const WEEKS = 53;
const LEVELS = ["bg-fill-strong", "bg-accent/30", "bg-accent/55", "bg-accent/80", "bg-accent"];
const monthFormat = new Intl.DateTimeFormat(undefined, { month: "short" });

/** A year of days, one column per week starting on Monday. */
export function Heatmap({ days, today = new Date() }: { days: DayActivity[]; today?: Date }) {
  const byDate = new Map(days.map((d) => [d.date, d]));
  const end = new Date(today);
  end.setHours(0, 0, 0, 0);
  const start = new Date(end);
  start.setDate(end.getDate() - ((end.getDay() + 6) % 7) - (WEEKS - 1) * 7);

  const words = days.map((d) => d.words).filter((w) => w > 0).sort((a, b) => a - b);
  const quantile = (p: number) => words[Math.floor((words.length - 1) * p)] ?? 0;
  const cuts = [quantile(0.25), quantile(0.5), quantile(0.75)];
  const level = (w: number) => (w <= 0 ? 0 : w <= cuts[0] ? 1 : w <= cuts[1] ? 2 : w <= cuts[2] ? 3 : 4);

  const cells: { date: Date; key: string; day?: DayActivity; future: boolean }[] = [];
  for (let i = 0; i < WEEKS * 7; i++) {
    const date = new Date(start);
    date.setDate(start.getDate() + i);
    const key = dayKey(date.getTime());
    cells.push({ date, key, day: byDate.get(key), future: date > end });
  }
  const months: { week: number; label: string }[] = [];
  for (let week = 0; week < WEEKS; week++) {
    const first = cells[week * 7].date;
    const previous = week > 0 ? cells[(week - 1) * 7].date : null;
    if (!previous || previous.getMonth() !== first.getMonth()) months.push({ week, label: monthFormat.format(first) });
  }

  return (
    <div>
      <div className="relative mb-1.5 h-3.5 text-[10.5px] text-ink-faint">
        {months
          // A month that only just starts in the first or last weeks has no room for its name.
          .filter((m, i) => (months[i + 1] ? months[i + 1].week - m.week >= 3 : m.week <= WEEKS - 3))
          .map((m) => (
            <span key={`${m.week}-${m.label}`} className="absolute" style={{ left: `${(m.week / WEEKS) * 100}%` }}>
              {m.label}
            </span>
          ))}
      </div>
      <div
        className="grid grid-flow-col grid-rows-7 gap-[3px]"
        style={{ gridTemplateColumns: `repeat(${WEEKS}, minmax(0, 1fr))` }}
        role="img"
        aria-label={`${days.length} days with dictations in the last year`}
      >
        {cells.map((cell) => (
          <span
            key={cell.key}
            title={
              cell.future
                ? undefined
                : `${formatShortDate(cell.date)}: ${cell.day ? plural(cell.day.words, "word") : "no dictations"}`
            }
            className={cn("aspect-square rounded-[2.5px]", cell.future ? "invisible" : LEVELS[level(cell.day?.words ?? 0)])}
          />
        ))}
      </div>
      <div className="mt-3 flex items-center justify-end gap-1.5 text-[10.5px] text-ink-faint">
        Less
        {LEVELS.map((c) => (
          <span key={c} className={cn("size-[10px] rounded-[2.5px]", c)} />
        ))}
        More
      </div>
    </div>
  );
}
