import { Copy, Pencil, Plus, Search, TextQuote, Trash } from "lucide-react";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input, SearchInput, Textarea } from "@/components/ui/input";
import { Card, EmptyState, PageHeader } from "@/components/ui/layout";
import { ROW_ACTIONS, RowAction } from "@/components/ui/row-action";
import { commands, type Snippet_Serialize as Snippet } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { formatNumber, plural } from "../format";
import { useLoad } from "../hooks";

/** Matches `MAX_SNIPPET` in the store. */
const MAX_SNIPPET = 4_000;

export function Snippets() {
  const [version, setVersion] = useState(0);
  const { data: snippets, error } = useLoad(() => commands.snippetsList(), [version]);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<Snippet | "new" | null>(null);
  const reload = () => setVersion((v) => v + 1);
  const matches = search(snippets ?? [], query);

  return (
    <>
      <PageHeader
        title="Snippets"
        description="Say a short cue like “my address” and Joona types the whole thing."
        actions={
          <Button variant="primary" onClick={() => setEditing("new")}>
            <Plus />
            Add snippet
          </Button>
        }
      />
      {error && !snippets && <p className="text-[13px] text-danger">{error}</p>}
      {snippets?.length === 0 && (
        <Card>
          <EmptyState icon={TextQuote} title="Say less, type more">
            Save things you type often, like your address, a sign-off or a meeting link. Then say a short phrase such as
            “my address” and Joona types it for you.
          </EmptyState>
        </Card>
      )}
      {snippets && snippets.length > 0 && (
        <section>
          <div className="mb-3 flex items-center justify-between gap-4">
            <p className="px-1 text-[12.5px] text-ink-soft">{plural(snippets.length, "snippet")}</p>
            <SearchInput className="w-64" value={query} onChange={setQuery} placeholder="Search snippets" />
          </div>
          {matches.length === 0 ? (
            <Card>
              <EmptyState icon={Search} title="Nothing matches">
                Try another word from the phrase or the text.
              </EmptyState>
            </Card>
          ) : (
            <Card className="overflow-clip">
              <ul className="divide-y divide-line">
                {matches.map((snippet) => (
                  <SnippetRow
                    key={snippet.id}
                    snippet={snippet}
                    onEdit={() => setEditing(snippet)}
                    onDeleted={reload}
                  />
                ))}
              </ul>
            </Card>
          )}
        </section>
      )}
      <SnippetDialog snippet={editing} onClose={() => setEditing(null)} onSaved={reload} />
    </>
  );
}

function search(snippets: Snippet[], query: string): Snippet[] {
  const q = query.trim().toLocaleLowerCase();
  if (!q) return snippets;
  return snippets.filter((s) => s.phrase.toLocaleLowerCase().includes(q) || s.text.toLocaleLowerCase().includes(q));
}

function SnippetRow({ snippet, onEdit, onDeleted }: { snippet: Snippet; onEdit: () => void; onDeleted: () => void }) {
  const [confirming, setConfirming] = useState(false);
  const copy = () =>
    void navigator.clipboard.writeText(snippet.text).then(
      () => toast.success("Copied"),
      () => toast.error("Couldn't copy it"),
    );
  const remove = () =>
    void commands.snippetDelete(snippet.id).then(
      () => {
        onDeleted();
        toast(`Deleted “${snippet.phrase}”`);
      },
      (e) => toast.error("Couldn't delete it", { description: String(e) }),
    );

  return (
    <li className="group relative px-4 py-3.5 transition-colors hover:bg-fill/50">
      <div className="flex items-baseline gap-2.5 pr-28">
        <span className="selectable text-[14px] font-medium text-ink">“{snippet.phrase}”</span>
        <span className="text-[12px] text-ink-faint tabular">
          {snippet.uses > 0 ? plural(snippet.uses, "use") : "Not used yet"}
        </span>
      </div>
      <p className="selectable mt-1 line-clamp-3 whitespace-pre-wrap break-words text-[13px] leading-relaxed text-ink-soft">
        {snippet.text}
      </p>
      <div className={ROW_ACTIONS}>
        <RowAction icon={Copy} label="Copy the text" onClick={copy} />
        <RowAction icon={Pencil} label="Edit" onClick={onEdit} />
        <RowAction icon={Trash} label="Delete" danger onClick={() => setConfirming(true)} />
      </div>
      <Confirm
        open={confirming}
        onOpenChange={setConfirming}
        title="Delete this snippet?"
        description={`Saying “${snippet.phrase}” will type just those words again.`}
        confirm="Delete"
        onConfirm={remove}
      />
    </li>
  );
}

function SnippetDialog({
  snippet,
  onClose,
  onSaved,
}: {
  snippet: Snippet | "new" | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const open = snippet !== null;
  const [wasOpen, setWasOpen] = useState(false);
  // What the dialog opened for, kept while it closes.
  const [shown, setShown] = useState<Snippet | "new">("new");
  const [phrase, setPhrase] = useState("");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (snippet !== null) {
      setShown(snippet);
      setPhrase(snippet === "new" ? "" : snippet.phrase);
      setText(snippet === "new" ? "" : snippet.text);
      setError(null);
    }
  }
  const editing = shown !== "new" ? shown : null;
  const length = [...text].length;
  const tooLong = length > MAX_SNIPPET;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    try {
      const saved = editing
        ? await commands.snippetUpdate(editing.id, phrase, text)
        : await commands.snippetAdd(phrase, text);
      onSaved();
      onClose();
      toast.success(editing ? "Saved" : `Added “${saved.phrase}”`);
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
      title={editing ? "Edit snippet" : "Add a snippet"}
      description="When you say the phrase, Joona types this text instead, exactly as written."
    >
      <form onSubmit={(e) => void submit(e)}>
        <Field
          id="snippet-phrase"
          label="When I say"
          hint="Say it on its own or in the middle of a sentence. A few words you wouldn't otherwise say together work best."
        >
          <Input
            id="snippet-phrase"
            autoFocus
            maxLength={60}
            spellCheck={false}
            value={phrase}
            placeholder="my address"
            onChange={(e) => {
              setPhrase(e.target.value);
              setError(null);
            }}
          />
        </Field>
        <Field
          id="snippet-text"
          label="Joona types"
          aside={
            <span className={cn("text-[12px] tabular", tooLong ? "font-medium text-danger" : "text-ink-faint")}>
              {formatNumber(length)} / {formatNumber(MAX_SNIPPET)}
            </span>
          }
        >
          <Textarea
            id="snippet-text"
            rows={6}
            value={text}
            placeholder={"Flat 4B, Lakeview Apartments\n12 MG Road, Bengaluru 560001"}
            onChange={(e) => {
              setText(e.target.value);
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
          <Button type="submit" variant="primary" disabled={busy || !phrase.trim() || !text.trim() || tooLong}>
            {editing ? "Save" : "Add"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
