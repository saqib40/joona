import { ClipboardPaste, Cloud, Copy, GitCompareArrows, History as HistoryIcon, LoaderCircle, Search, Trash } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { SearchInput } from "@/components/ui/input";
import { Keys } from "@/components/ui/kbd";
import { Card, EmptyState } from "@/components/ui/layout";
import { ROW_ACTIONS, RowAction } from "@/components/ui/row-action";
import { commands } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { dayKey, dayLabel, formatClip, formatTime, plural } from "../../format";
import { type Entry, groupByDay, useHistory, wasChanged } from "../../history";
import { routeLabel } from "../../languages";
import { shortcutKeys } from "../../shortcut";
import { useHub } from "../../store";

export function HistoryList() {
  const entries = useHistory((s) => s.entries);
  const loading = useHistory((s) => s.loading);
  const done = useHistory((s) => s.done);
  const error = useHistory((s) => s.error);
  const search = useHistory((s) => s.search);
  const keep = useHub((s) => s.settings?.privacy.keepHistory ?? true);
  const [query, setQuery] = useState(search);
  const end = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const t = window.setTimeout(() => {
      if (query !== useHistory.getState().search) useHistory.getState().setSearch(query);
    }, 200);
    return () => window.clearTimeout(t);
  }, [query]);

  useEffect(() => {
    const node = end.current;
    if (!node || done) return;
    const observer = new IntersectionObserver((seen) => {
      if (seen.some((e) => e.isIntersecting)) void useHistory.getState().loadMore();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [done, entries.length]);

  const groups = groupByDay(entries, dayKey);
  return (
    <section className="mt-10">
      <div className="mb-3 flex items-center justify-between gap-4">
        <h2 className="px-1 text-[13px] font-semibold text-ink">History</h2>
        <SearchInput className="w-64" value={query} onChange={setQuery} placeholder="Search your dictations" />
      </div>

      {error && <p className="px-1 text-[13px] text-danger">{error}</p>}
      {entries.length === 0 && !loading && !error && <Empty searching={search.trim() !== ""} keep={keep} />}

      <div className="space-y-6">
        {groups.map(([key, items]) => (
          <div key={key}>
            <h3 className="mb-2 px-1 text-[11.5px] font-semibold uppercase tracking-[0.07em] text-ink-faint">
              {dayLabel(items[0].createdAt)}
            </h3>
            <Card className="overflow-clip">
              <ul className="divide-y divide-line">
                {items.map((entry) => (
                  <Row key={entry.id} entry={entry} />
                ))}
              </ul>
            </Card>
          </div>
        ))}
      </div>
      <div ref={end} className="h-6" />
      {loading && (
        <div className="flex justify-center py-4 text-ink-faint">
          <LoaderCircle className="size-4 animate-spin" />
        </div>
      )}
    </section>
  );
}

function Empty({ searching, keep }: { searching: boolean; keep: boolean }) {
  const trigger = useHub((s) => s.settings?.shortcuts.trigger);
  if (searching) {
    return (
      <Card>
        <EmptyState icon={Search} title="Nothing matches">
          Try fewer words, or the name of the app you dictated into.
        </EmptyState>
      </Card>
    );
  }
  return (
    <Card>
      <EmptyState icon={HistoryIcon} title={keep ? "Your dictations will show up here" : "History is off"}>
        {keep ? (
          <>
            Hold {trigger ? <Keys keys={shortcutKeys(trigger)} className="mx-0.5" /> : "the shortcut"} in any app and
            start talking. Everything is kept on this PC.
          </>
        ) : (
          "Turn on Keep history in Settings → Data & Privacy to search past dictations."
        )}
      </EmptyState>
    </Card>
  );
}

function Row({ entry }: { entry: Entry }) {
  const [expanded, setExpanded] = useState(false);
  const [compare, setCompare] = useState(false);
  const changed = wasChanged(entry);
  const route = routeLabel(entry.language, entry.output);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(entry.text);
      toast.success("Copied");
    } catch (e) {
      toast.error("Couldn't copy", { description: String(e) });
    }
  };
  const paste = () =>
    void commands.historyPaste(entry.id).catch((e) => toast.error("Couldn't paste", { description: String(e) }));
  const remove = () =>
    void useHistory
      .getState()
      .remove(entry.id)
      .then(
        () => toast("Deleted from history"),
        (e) => toast.error("Couldn't delete", { description: String(e) }),
      );

  return (
    <li className="group relative grid grid-cols-[72px_minmax(0,1fr)] gap-4 px-4 py-3.5 transition-colors hover:bg-fill/50">
      <time className="pt-[2px] text-[12px] text-ink-faint tabular">{formatTime(entry.createdAt)}</time>
      <div className="min-w-0">
        <p
          onDoubleClick={() => setExpanded((x) => !x)}
          className={cn(
            "selectable whitespace-pre-wrap break-words pr-2 text-[14px] leading-relaxed text-ink",
            !expanded && "line-clamp-3",
          )}
        >
          {entry.text}
        </p>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-ink-faint">
          {[entry.appName, route, plural(entry.words, "word")].filter(Boolean).map((part, i) => (
            <span key={i} className="flex items-center gap-2">
              {i > 0 && <span className="size-[3px] rounded-full bg-line-strong" />}
              {part}
            </span>
          ))}
          {entry.cloud && (
            <Badge tone="cloud">
              <Cloud />
              {entry.cloud}
            </Badge>
          )}
          {entry.mode === "command" && <Badge>Command</Badge>}
          {entry.delivery === "copied" && <Badge>Left on clipboard</Badge>}
          {entry.delivery === "failed" && <Badge tone="danger">Not inserted</Badge>}
        </div>
        {compare && (
          <div className="mt-3 grid grid-cols-2 gap-4 rounded-lg bg-fill/70 p-3">
            <Version title="Heard" text={entry.raw} />
            <Version title="Typed" text={entry.text} />
            <p className="col-span-2 text-[11.5px] text-ink-faint">
              {entry.model} · {formatClip(entry.audioMs)}
              {entry.fixes > 0 && ` · ${plural(entry.fixes, "fix", "fixes")}`}
            </p>
          </div>
        )}
      </div>
      <div className={ROW_ACTIONS}>
        <RowAction icon={Copy} label="Copy" onClick={() => void copy()} />
        <RowAction icon={ClipboardPaste} label="Paste into the last app" onClick={paste} />
        {changed && (
          <RowAction
            icon={GitCompareArrows}
            label={compare ? "Hide what was heard" : "Compare with what was heard"}
            active={compare}
            onClick={() => setCompare((x) => !x)}
          />
        )}
        <RowAction icon={Trash} label="Delete" danger onClick={remove} />
      </div>
    </li>
  );
}

function Version({ title, text }: { title: string; text: string }) {
  return (
    <div className="min-w-0">
      <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-[0.07em] text-ink-faint">{title}</div>
      <p className="selectable whitespace-pre-wrap break-words text-[12.5px] leading-relaxed text-ink-soft">{text}</p>
    </div>
  );
}
