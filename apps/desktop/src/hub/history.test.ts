import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { type Entry, groupByDay, useHistory, wasChanged } from "./history";

const entry = (id: number, extra: Partial<Entry> = {}): Entry => ({
  id,
  createdAt: 1_000_000 + id * 1000,
  app: "slack.exe",
  appName: "Slack",
  raw: `note ${id}`,
  text: `Note ${id}`,
  language: "en",
  output: null,
  words: 2,
  audioMs: 1500,
  model: "Whisper Base",
  cloud: null,
  mode: "dictate",
  delivery: "pasted",
  fixes: 0,
  category: null,
  audio: false,
  ...extra,
});

/** Newest first, like the Rust store. */
const all = Array.from({ length: 75 }, (_, i) => entry(75 - i));

function serve(entries: Entry[], delay = (_search: string) => 0) {
  const calls: { search: string; before: number | null }[] = [];
  mockIPC(async (cmd, args) => {
    if (cmd !== "history_list") return null;
    const { search, before, beforeId, limit } = args as {
      search: string;
      before: number | null;
      beforeId: number | null;
      limit: number;
    };
    calls.push({ search, before });
    await new Promise((r) => setTimeout(r, delay(search)));
    return entries
      .filter((e) => {
        if (before === null) return true;
        if (e.createdAt < before) return true;
        return beforeId !== null && e.createdAt === before && e.id < beforeId;
      })
      .filter((e) => e.text.toLowerCase().includes(search.toLowerCase()))
      .slice(0, limit);
  });
  return calls;
}

describe("history store", () => {
  beforeEach(() => useHistory.setState({ search: "", entries: [], loading: false, done: false, error: null, version: 0 }));
  afterEach(() => clearMocks());

  it("loads a page at a time", async () => {
    const calls = serve(all);
    await useHistory.getState().reload();
    expect(useHistory.getState().entries).toHaveLength(60);
    expect(useHistory.getState().done).toBe(false);

    await useHistory.getState().loadMore();
    expect(calls[1].before).toBe(all[59].createdAt);
    expect(useHistory.getState().entries.map((e) => e.id)).toEqual(all.map((e) => e.id));
    expect(useHistory.getState().done).toBe(true);

    await useHistory.getState().loadMore();
    expect(calls).toHaveLength(2);
  });

  it("drops an answer to an older search", async () => {
    serve(all, (search) => (search === "note 7" ? 30 : 0));
    useHistory.getState().setSearch("note 7");
    useHistory.getState().setSearch("note 12");
    await new Promise((r) => setTimeout(r, 60));
    expect(useHistory.getState().entries.map((e) => e.id)).toEqual([12]);
  });

  it("adds new dictations that match the search", () => {
    useHistory.setState({ search: "lunch", entries: [] });
    useHistory.getState().added(entry(100, { text: "Lunch at one?" }));
    useHistory.getState().added(entry(100, { text: "Lunch at one?" }));
    useHistory.getState().added(entry(101, { text: "Call me back" }));
    expect(useHistory.getState().entries.map((e) => e.id)).toEqual([100]);
    expect(useHistory.getState().version).toBe(3);
  });

  it("puts an entry back when deleting fails", async () => {
    mockIPC((cmd) => {
      if (cmd === "history_delete") throw new Error("locked");
    });
    useHistory.setState({ entries: [entry(1), entry(2)] });
    await expect(useHistory.getState().remove(1)).rejects.toBeTruthy();
    expect(useHistory.getState().entries.map((e) => e.id)).toEqual([1, 2]);
  });
});

describe("history helpers", () => {
  it("tells when cleanup changed the words", () => {
    expect(wasChanged(entry(1, { raw: "hello  world", text: "Hello world" }))).toBe(false);
    expect(wasChanged(entry(1, { raw: "um hello world", text: "Hello world." }))).toBe(true);
  });

  it("groups entries by day in order", () => {
    const entries = [entry(3, { createdAt: 30 }), entry(2, { createdAt: 20 }), entry(1, { createdAt: 10 })];
    const groups = groupByDay(entries, (ms) => (ms >= 20 ? "today" : "yesterday"));
    expect(groups.map(([day, list]) => [day, list.map((e) => e.id)])).toEqual([
      ["today", [3, 2]],
      ["yesterday", [1]],
    ]);
  });
});
