import { create } from "zustand";
import { commands, type Entry_Serialize as Entry } from "@/lib/bindings";

export type { Entry };

const PAGE = 60;

interface HistoryStore {
  search: string;
  entries: Entry[];
  loading: boolean;
  /** Every entry matching the search is loaded. */
  done: boolean;
  error: string | null;
  /** Bumped whenever the history changes, so the numbers on Home and Insights refresh. */
  version: number;
  setSearch: (search: string) => void;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  added: (entry: Entry) => void;
  remove: (id: number) => Promise<void>;
}

let request = 0;

export const useHistory = create<HistoryStore>((set, get) => ({
  search: "",
  entries: [],
  loading: false,
  done: false,
  error: null,
  version: 0,
  setSearch: (search) => {
    set({ search });
    void get().reload();
  },
  reload: async () => {
    set({ entries: [], done: false, error: null });
    await load(set, get, true);
  },
  loadMore: async () => {
    const { loading, done } = get();
    if (loading || done) return;
    await load(set, get, false);
  },
  added: (entry) =>
    set((s) => ({
      version: s.version + 1,
      entries: matches(entry, s.search) && !s.entries.some((e) => e.id === entry.id) ? [entry, ...s.entries] : s.entries,
    })),
  remove: async (id) => {
    const before = get().entries;
    set((s) => ({ entries: s.entries.filter((e) => e.id !== id) }));
    try {
      await commands.historyDelete(id);
      set((s) => ({ version: s.version + 1 }));
    } catch (e) {
      set({ entries: before });
      throw e;
    }
  },
}));

type Set = (partial: Partial<HistoryStore> | ((s: HistoryStore) => Partial<HistoryStore>)) => void;

async function load(set: Set, get: () => HistoryStore, fresh: boolean) {
  const mine = ++request;
  const { search, entries } = get();
  const before = fresh ? null : (entries.at(-1)?.createdAt ?? null);
  set({ loading: true });
  try {
    const page = await commands.historyList(search, before, PAGE);
    if (mine !== request) return;
    set((s) => ({
      entries: fresh ? page : [...s.entries, ...page.filter((e) => !s.entries.some((x) => x.id === e.id))],
      done: page.length < PAGE,
      loading: false,
    }));
  } catch (e) {
    if (mine !== request) return;
    set({ loading: false, error: String(e), done: true });
  }
}

function matches(entry: Entry, search: string): boolean {
  const q = search.trim().toLowerCase();
  if (!q) return true;
  return [entry.text, entry.raw, entry.appName ?? ""].some((t) => t.toLowerCase().includes(q));
}

/** Words that differ between what was heard and what went in, ignoring spacing and case. */
export function wasChanged(entry: Entry): boolean {
  const norm = (s: string) => s.replace(/\s+/g, " ").trim().toLowerCase();
  return norm(entry.raw) !== norm(entry.text);
}

/** Entries grouped by local day, newest first. */
export function groupByDay(entries: Entry[], key: (ms: number) => string): [string, Entry[]][] {
  const groups = new Map<string, Entry[]>();
  for (const entry of entries) {
    const k = key(entry.createdAt);
    const group = groups.get(k);
    if (group) group.push(entry);
    else groups.set(k, [entry]);
  }
  return [...groups.entries()];
}
