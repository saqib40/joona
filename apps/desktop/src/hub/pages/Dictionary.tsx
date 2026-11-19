import { open, save } from "@tauri-apps/plugin-dialog";
import { BookA, FileDown, FileUp, Pencil, Plus, Search, Star, Trash } from "lucide-react";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input, SearchInput } from "@/components/ui/input";
import { Card, EmptyState, PageHeader } from "@/components/ui/layout";
import { ROW_ACTIONS, RowAction } from "@/components/ui/row-action";
import { Tooltip } from "@/components/ui/tooltip";
import { commands, type DictionaryEntry_Serialize as DictionaryEntry } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { formatNumber, plural } from "../format";
import { useLoad } from "../hooks";

/** Rows shown at once. Search finds the rest. */
const SHOWN = 200;

export function Dictionary() {
  const [version, setVersion] = useState(0);
  const { data: entries, error } = useLoad(() => commands.dictionaryList(), [version]);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<DictionaryEntry | "new" | null>(null);
  const reload = () => setVersion((v) => v + 1);
  const matches = search(entries ?? [], query);

  return (
    <>
      <PageHeader
        title="Dictionary"
        description="Names, product terms and words from your languages, spelled right every time."
        actions={
          <Button variant="primary" onClick={() => setEditing("new")}>
            <Plus />
            Add word
          </Button>
        }
      />
      {error && !entries && <p className="text-[13px] text-danger">{error}</p>}
      {entries?.length === 0 && (
        <Card>
          <EmptyState
            icon={BookA}
            title="Teach Joona your words"
            action={
              <Button onClick={() => void importWords(reload)}>
                <FileUp />
                Import a CSV file
              </Button>
            }
          >
            Add names, brands and terms the speech model gets wrong. Joona listens for them and spells them your way.
          </EmptyState>
        </Card>
      )}
      {entries && entries.length > 0 && (
        <section>
          <div className="mb-3 flex items-center justify-between gap-4">
            <p className="px-1 text-[12.5px] text-ink-soft">{plural(entries.length, "entry", "entries")}</p>
            <div className="flex items-center gap-2">
              <SearchInput className="w-60" value={query} onChange={setQuery} placeholder="Search your dictionary" />
              <Tooltip content="Import words from a CSV file">
                <Button size="icon" aria-label="Import" onClick={() => void importWords(reload)}>
                  <FileUp />
                </Button>
              </Tooltip>
              <Tooltip content="Export to a CSV file">
                <Button size="icon" aria-label="Export" onClick={() => void exportWords()}>
                  <FileDown />
                </Button>
              </Tooltip>
            </div>
          </div>
          {matches.length === 0 ? (
            <Card>
              <EmptyState icon={Search} title="Nothing matches">
                Try another spelling, or add it as a new word.
              </EmptyState>
            </Card>
          ) : (
            <Card className="overflow-clip">
              <ul className="divide-y divide-line">
                {matches.slice(0, SHOWN).map((entry) => (
                  <EntryRow key={entry.id} entry={entry} onEdit={() => setEditing(entry)} onChanged={reload} />
                ))}
              </ul>
            </Card>
          )}
          {matches.length > SHOWN && (
            <p className="mt-3 px-1 text-[12px] text-ink-faint">
              Showing {formatNumber(SHOWN)} of {formatNumber(matches.length)}. Search to find the rest.
            </p>
          )}
        </section>
      )}
      <EntryDialog entry={editing} onClose={() => setEditing(null)} onSaved={reload} />
    </>
  );
}

function search(entries: DictionaryEntry[], query: string): DictionaryEntry[] {
  const q = query.trim().toLocaleLowerCase();
  if (!q) return entries;
  return entries.filter((e) => e.word.toLocaleLowerCase().includes(q) || e.heard?.toLocaleLowerCase().includes(q));
}

async function importWords(onImported: () => void) {
  const path = await open({
    title: "Import words",
    multiple: false,
    directory: false,
    filters: [{ name: "Spreadsheet", extensions: ["csv", "txt"] }],
  });
  if (!path) return;
  try {
    const { added, skipped } = await commands.dictionaryImport(path);
    onImported();
    toast.success(added > 0 ? `Added ${plural(added, "word")}` : "No new words", {
      description: skipped > 0 ? `Skipped ${plural(skipped, "row")}: already there, empty or too long.` : undefined,
    });
  } catch (e) {
    toast.error("Couldn't import", { description: String(e) });
  }
}

async function exportWords() {
  const stamp = new Date().toISOString().slice(0, 10);
  const path = await save({
    title: "Export dictionary",
    defaultPath: `joona-dictionary-${stamp}.csv`,
    filters: [{ name: "Spreadsheet", extensions: ["csv"] }],
  });
  if (!path) return;
  try {
    const count = await commands.dictionaryExport(path);
    toast.success(`Exported ${plural(count, "entry", "entries")}`);
  } catch (e) {
    toast.error("Couldn't export", { description: String(e) });
  }
}

function EntryRow({ entry, onEdit, onChanged }: { entry: DictionaryEntry; onEdit: () => void; onChanged: () => void }) {
  const star = () =>
    void commands.dictionaryStar(entry.id, !entry.starred).then(onChanged, (e) =>
      toast.error("Couldn't change the star", { description: String(e) }),
    );
  const remove = () =>
    void commands.dictionaryDelete(entry.id).then(
      () => {
        onChanged();
        toast(`Deleted “${entry.word}”`);
      },
      (e) => toast.error("Couldn't delete it", { description: String(e) }),
    );

  return (
    <li className="group relative flex min-h-[52px] items-center gap-3 px-4 py-2.5 transition-colors hover:bg-fill/50">
      <Tooltip content={entry.starred ? "Unstar" : "Star to keep it at the top, where the speech model hears about it first"}>
        <button
          type="button"
          aria-label={entry.starred ? `Unstar ${entry.word}` : `Star ${entry.word}`}
          aria-pressed={entry.starred}
          onClick={star}
          className={cn(
            "grid size-7 shrink-0 place-items-center rounded-md transition-colors hover:bg-fill",
            entry.starred ? "text-accent-ink" : "text-ink-faint hover:text-ink",
          )}
        >
          <Star className="size-4" fill={entry.starred ? "currentColor" : "none"} strokeWidth={1.8} />
        </button>
      </Tooltip>
      <div className="min-w-0 flex-1">
        <p className="selectable truncate text-[14px] font-medium text-ink">{entry.word}</p>
        {entry.heard && <p className="mt-0.5 truncate text-[12.5px] text-ink-soft">Fixes “{entry.heard}”</p>}
      </div>
      <span className="shrink-0 text-[12px] text-ink-faint tabular group-focus-within:invisible group-hover:invisible">
        {entry.uses > 0 ? plural(entry.uses, "use") : "Not used yet"}
      </span>
      <div className={cn(ROW_ACTIONS, "top-1/2 -translate-y-1/2")}>
        <RowAction icon={Pencil} label="Edit" onClick={onEdit} />
        <RowAction icon={Trash} label="Delete" danger onClick={remove} />
      </div>
    </li>
  );
}

function EntryDialog({
  entry,
  onClose,
  onSaved,
}: {
  entry: DictionaryEntry | "new" | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const open = entry !== null;
  const [wasOpen, setWasOpen] = useState(false);
  // What the dialog opened for, kept while it closes.
  const [shown, setShown] = useState<DictionaryEntry | "new">("new");
  const [word, setWord] = useState("");
  const [heard, setHeard] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (entry !== null) {
      setShown(entry);
      setWord(entry === "new" ? "" : entry.word);
      setHeard(entry === "new" ? "" : (entry.heard ?? ""));
      setError(null);
    }
  }
  const editing = shown !== "new" ? shown : null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    try {
      const wrong = heard.trim() ? heard : null;
      const saved = editing
        ? await commands.dictionaryUpdate(editing.id, word, wrong)
        : await commands.dictionaryAdd(word, wrong);
      onSaved();
      onClose();
      toast.success(editing ? "Saved" : `Added “${saved.word}”`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && onClose()}
      title={editing ? "Edit entry" : "Add to the dictionary"}
      description="Joona listens for it and writes it exactly like this."
    >
      <form onSubmit={(e) => void submit(e)}>
        <Field id="dictionary-word" label="Word or phrase">
          <Input
            id="dictionary-word"
            autoFocus
            maxLength={100}
            spellCheck={false}
            value={word}
            placeholder="Priya Sharma, Kubernetes, Thiruvananthapuram"
            onChange={(e) => {
              setWord(e.target.value);
              setError(null);
            }}
          />
        </Field>
        <Field
          id="dictionary-heard"
          label="Written wrong as"
          aside={<span className="text-[12px] text-ink-faint">Optional</span>}
          hint="If Joona keeps writing it another way, type that here and it gets swapped every time."
        >
          <Input
            id="dictionary-heard"
            maxLength={100}
            spellCheck={false}
            value={heard}
            placeholder="Priya Sharmaa"
            onChange={(e) => {
              setHeard(e.target.value);
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
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={busy || !word.trim()}>
            {editing ? "Save" : "Add"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
