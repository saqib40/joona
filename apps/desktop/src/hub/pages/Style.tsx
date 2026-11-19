import { Globe, Plus } from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Group, PageHeader, Row } from "@/components/ui/layout";
import { Segmented } from "@/components/ui/segmented";
import { Select } from "@/components/ui/select";
import {
  type AppRule,
  type Category,
  type CategoryStyle,
  commands,
  type RecentApp,
  type StyleOverview,
  type StyleTone,
} from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { saveSettings } from "../settings";
import { useHub } from "../store";

const CATEGORIES: { value: Category; label: string; description: string }[] = [
  {
    value: "personal",
    label: "Personal messages",
    description: "Chats with friends and family, such as WhatsApp and Telegram.",
  },
  { value: "work", label: "Work messages", description: "Chats at work, such as Slack and Teams." },
  { value: "email", label: "Email", description: "Email apps and websites, such as Outlook and Gmail." },
  {
    value: "other",
    label: "Other",
    description: "Documents, notes and every app or website that isn't in another category.",
  },
];

const TONES: { value: StyleTone; label: string; description: string }[] = [
  { value: "formal", label: "Formal", description: "Capitals, full punctuation" },
  { value: "casual", label: "Casual", description: "Capitals, light punctuation" },
  { value: "veryCasual", label: "Very casual", description: "No capitals, light punctuation" },
  { value: "excited", label: "Excited", description: "More exclamation marks" },
];

const CATEGORY_OPTIONS = CATEGORIES.map(({ value, label }) => ({ value, label }));

function categoryLabel(category: Category): string {
  return CATEGORIES.find((c) => c.value === category)?.label ?? category;
}

export function Style() {
  const polish = useHub((s) => s.settings?.polish.level);
  const [overview, setOverview] = useState<StyleOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [category, setCategory] = useState<Category>("personal");
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    let live = true;
    commands.styleOverview().then(
      (o) => live && setOverview(o),
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, []);

  const change = (update: Promise<StyleOverview>, failure: string) =>
    void update.then(setOverview, (e) => toast.error(failure, { description: String(e) }));
  const current = overview?.categories.find((c) => c.category === category);
  const about = CATEGORIES.find((c) => c.value === category);

  return (
    <>
      <PageHeader
        title="Style"
        description="How formal your writing sounds in each kind of app. Joona picks the style from the app you're dictating into."
      />
      {polish === "off" && (
        <Group className="mb-6">
          <Row title="Styles need polish" description="Polish is off, so Joona types your words just as it heard them.">
            <Button variant="primary" onClick={() => void saveSettings((s) => void (s.polish.level = "basic"))}>
              Turn on Basic polish
            </Button>
          </Row>
        </Group>
      )}
      {error && !overview && <p className="text-[13px] text-danger">{error}</p>}
      {overview && current && (
        <>
          <Segmented label="Kind of app" value={category} onChange={setCategory} options={CATEGORY_OPTIONS} />
          <p className="mt-3 px-1 text-[12.5px] text-ink-soft">{about?.description}</p>
          <Tones
            style={current}
            onChange={(tone) => change(commands.styleSetTone(category, tone), "Couldn't change the style")}
          />
          <Apps
            category={category}
            apps={overview.apps.filter((a) => a.category === category)}
            onAdd={() => setAdding(true)}
            onMove={(app, to) => change(commands.styleSetApp(app, to), "Couldn't move it")}
          />
          <AddDialog
            open={adding}
            onOpenChange={setAdding}
            category={category}
            recent={overview.recent}
            onAdd={async (app) => setOverview(await commands.styleSetApp(app, category))}
          />
        </>
      )}
    </>
  );
}

function Tones({ style, onChange }: { style: CategoryStyle; onChange: (tone: StyleTone) => void }) {
  return (
    <div
      role="radiogroup"
      aria-label={`Style for ${categoryLabel(style.category)}`}
      className="mt-4 grid grid-cols-2 gap-3"
    >
      {TONES.map((tone) => {
        const checked = tone.value === style.tone;
        const preview = style.previews.find((p) => p.tone === tone.value)?.text;
        return (
          <button
            key={tone.value}
            type="button"
            role="radio"
            aria-checked={checked}
            onClick={() => onChange(tone.value)}
            className={cn(
              "flex flex-col rounded-xl bg-surface-raised p-4 text-left shadow-soft ring-1 transition-[box-shadow,background-color]",
              checked ? "ring-2 ring-ink" : "ring-line hover:bg-fill/60",
            )}
          >
            <span className="flex items-start justify-between gap-3">
              <span>
                <span className="block text-[14px] font-semibold text-ink">{tone.label}</span>
                <span className="mt-0.5 block text-[12px] text-ink-faint">{tone.description}</span>
              </span>
              <span
                className={cn(
                  "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full ring-1 ring-line-strong",
                  checked && "bg-ink ring-ink",
                )}
              >
                {checked && <span className="size-1.5 rounded-full bg-surface-raised" />}
              </span>
            </span>
            <span className="mt-3.5 flex-1 rounded-[14px] rounded-bl-[4px] bg-fill px-3.5 py-2.5 text-[13px] leading-relaxed text-ink">
              {preview}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function Apps({
  category,
  apps,
  onAdd,
  onMove,
}: {
  category: Category;
  apps: AppRule[];
  onAdd: () => void;
  onMove: (app: string, to: Category | null) => void;
}) {
  return (
    <Group
      className="mt-8"
      title="Apps and websites"
      description={
        category === "other"
          ? "Anything not listed in another category uses this style too."
          : `Joona uses the ${categoryLabel(category).toLowerCase()} style in these.`
      }
      action={
        <Button size="sm" onClick={onAdd}>
          <Plus />
          Add
        </Button>
      }
    >
      {apps.length === 0 ? (
        <Row
          title="Nothing here yet"
          description="Add an app you dictate into, or a website you use in your browser."
        />
      ) : (
        apps.map((rule) => <AppRow key={rule.app} rule={rule} onMove={(to) => onMove(rule.app, to)} />)
      )}
    </Group>
  );
}

function AppRow({ rule, onMove }: { rule: AppRule; onMove: (to: Category | null) => void }) {
  return (
    <Row
      title={
        <span className="flex items-center gap-2">
          <span className="truncate">{rule.name}</span>
          {rule.website && (
            <Badge>
              <Globe />
              Website
            </Badge>
          )}
        </span>
      }
      description={rule.website ? "Found by the browser tab's title" : rule.app}
    >
      {rule.changed && (
        <Button variant="ghost" size="sm" onClick={() => onMove(null)}>
          {rule.builtIn ? "Reset" : "Remove"}
        </Button>
      )}
      <Select
        label={`Category for ${rule.name}`}
        value={rule.category}
        onChange={(to) => onMove(to)}
        options={CATEGORY_OPTIONS}
        className="min-w-44"
      />
    </Row>
  );
}

function AddDialog({
  open,
  onOpenChange,
  category,
  recent,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  category: Category;
  recent: RecentApp[];
  onAdd: (app: string) => Promise<void>;
}) {
  const [wasOpen, setWasOpen] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setName("");
      setError(null);
    }
  }

  const add = async (app: string) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onAdd(app);
      onOpenChange(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void add(name);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={`Add to ${categoryLabel(category)}`}
      description="Pick an app you've dictated into, or type the name of a website."
    >
      {recent.length > 0 && (
        <div className="mt-5">
          <p className="text-[13px] font-medium text-ink">Apps you've used</p>
          <div className="mt-2 flex flex-wrap gap-2">
            {recent.map((app) => (
              <button
                key={app.app}
                type="button"
                disabled={busy}
                onClick={() => void add(app.app)}
                className="inline-flex h-8 items-center rounded-lg bg-fill px-3 text-[13px] text-ink transition-colors hover:bg-fill-strong disabled:opacity-45"
              >
                {app.name}
              </button>
            ))}
          </div>
        </div>
      )}
      <form onSubmit={submit}>
        <Field
          id="style-website"
          label="Website"
          hint="Joona looks for the name in the browser tab's title. You can also type a program, such as notion.exe."
        >
          <Input
            id="style-website"
            autoFocus={recent.length === 0}
            maxLength={60}
            spellCheck={false}
            value={name}
            placeholder="Google Docs"
            onChange={(e) => {
              setName(e.target.value);
              setError(null);
            }}
          />
        </Field>
        {error && (
          <p role="alert" className="mt-4 text-[12.5px] text-danger">
            {error}
          </p>
        )}
        <div className="mt-6 flex justify-end gap-2">
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={busy || !name.trim()}>
            Add
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
