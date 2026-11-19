import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { DictionaryEntry_Serialize as DictionaryEntry } from "@/lib/bindings";
import { fakeHub } from "../testing";
import { Dictionary } from "./Dictionary";

vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => "C:\\Users\\asha\\words.csv"),
  save: vi.fn(async () => "C:\\Users\\asha\\dictionary.csv"),
}));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
});

function entry(id: number, word: string, extra: Partial<DictionaryEntry> = {}): DictionaryEntry {
  return { id, word, heard: null, starred: false, uses: 0, lastUsed: null, createdAt: 0, ...extra };
}

/** A fake dictionary in Rust, and the changes made to it. */
function fakeDictionary(start: DictionaryEntry[]) {
  let entries = start;
  const calls: [string, Record<string, unknown>][] = [];
  fakeHub(undefined, (cmd, args) => {
    if (!cmd.startsWith("dictionary_")) return undefined;
    if (cmd === "dictionary_list") return entries;
    calls.push([cmd, args]);
    switch (cmd) {
      case "dictionary_add": {
        const added = entry(100, args.word as string, { heard: args.heard as string | null });
        entries = [...entries, added];
        return added;
      }
      case "dictionary_update":
        entries = entries.map((e) =>
          e.id === args.id ? { ...e, word: args.word as string, heard: args.heard as string | null } : e,
        );
        return entries.find((e) => e.id === args.id);
      case "dictionary_star":
        entries = entries.map((e) => (e.id === args.id ? { ...e, starred: args.starred as boolean } : e));
        return null;
      case "dictionary_delete":
        entries = entries.filter((e) => e.id !== args.id);
        return null;
      case "dictionary_import":
        entries = [...entries, entry(101, "Figma"), entry(102, "Supabase")];
        return { added: 2, skipped: 1 };
      default:
        return null;
    }
  });
  return calls;
}

const show = () => render(<Dictionary />, { wrapper: TooltipProvider });
const row = (word: string) => screen.getByText(word).closest("li") as HTMLElement;

describe("Dictionary", () => {
  it("lists words with their fixes and adds one", async () => {
    const calls = fakeDictionary([entry(1, "Joona", { heard: "Juna", starred: true, uses: 31 }), entry(2, "GitHub")]);
    show();
    expect(await screen.findByText("Joona")).toBeInTheDocument();
    expect(screen.getByText("Fixes “Juna”")).toBeInTheDocument();
    expect(screen.getByText("31 uses")).toBeInTheDocument();
    expect(screen.getByText("Not used yet")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Unstar Joona" })).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(screen.getByRole("button", { name: "Add word" }));
    const dialog = await screen.findByRole("dialog", { name: "Add to the dictionary" });
    await userEvent.type(within(dialog).getByLabelText("Word or phrase"), "Priya Sharma");
    await userEvent.type(within(dialog).getByLabelText("Written wrong as"), "Pria Sharma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Add" }));

    expect(await screen.findByText("Priya Sharma")).toBeInTheDocument();
    expect(calls).toEqual([["dictionary_add", { word: "Priya Sharma", heard: "Pria Sharma" }]]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(toast.success).toHaveBeenCalledWith("Added “Priya Sharma”");
  });

  it("shows why Rust refused a word until it's changed", async () => {
    fakeHub(undefined, (cmd) => {
      if (cmd === "dictionary_list") return [entry(1, "GitHub")];
      if (cmd === "dictionary_add") throw "“GitHub” is already in your dictionary.";
      return undefined;
    });
    show();
    await userEvent.click(await screen.findByRole("button", { name: "Add word" }));
    const dialog = await screen.findByRole("dialog", { name: "Add to the dictionary" });
    const word = within(dialog).getByLabelText("Word or phrase");
    await userEvent.type(word, "github");
    await userEvent.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("“GitHub” is already in your dictionary.");

    await userEvent.type(word, "s");
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
  });

  it("stars, edits and deletes words", async () => {
    const calls = fakeDictionary([entry(1, "Joona", { heard: "Juna" }), entry(2, "GitHub")]);
    show();
    await userEvent.click(await screen.findByRole("button", { name: "Star GitHub" }));
    expect(await screen.findByRole("button", { name: "Unstar GitHub" })).toBeInTheDocument();

    await userEvent.click(within(row("Joona")).getByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit entry" });
    expect(within(dialog).getByLabelText("Word or phrase")).toHaveValue("Joona");
    await userEvent.clear(within(dialog).getByLabelText("Written wrong as"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await vi.waitFor(() => expect(screen.queryByText("Fixes “Juna”")).not.toBeInTheDocument());

    await userEvent.click(within(row("GitHub")).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(screen.queryByText("GitHub")).not.toBeInTheDocument());
    expect(toast).toHaveBeenCalledWith("Deleted “GitHub”");
    expect(calls).toEqual([
      ["dictionary_star", { id: 2, starred: true }],
      ["dictionary_update", { id: 1, word: "Joona", heard: null }],
      ["dictionary_delete", { id: 2 }],
    ]);
  });

  it("opens an edit with what's saved, even after a cancelled change", async () => {
    fakeDictionary([entry(1, "Joona")]);
    show();
    await screen.findByText("Joona");
    const edit = async () => {
      await userEvent.click(within(row("Joona")).getByRole("button", { name: "Edit" }));
      return screen.findByRole("dialog", { name: "Edit entry" });
    };
    const first = await edit();
    await userEvent.type(within(first).getByLabelText("Word or phrase"), " Bar");
    await userEvent.click(within(first).getByRole("button", { name: "Cancel" }));
    const second = await edit();
    expect(within(second).getByLabelText("Word or phrase")).toHaveValue("Joona");
  });

  it("imports a CSV file and searches the list", async () => {
    const calls = fakeDictionary([]);
    show();
    expect(await screen.findByText("Teach Joona your words")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Import a CSV file" }));
    expect(await screen.findByText("Figma")).toBeInTheDocument();
    expect(calls).toEqual([["dictionary_import", { path: "C:\\Users\\asha\\words.csv" }]]);
    expect(toast.success).toHaveBeenCalledWith("Added 2 words", {
      description: "Skipped 1 row: already there, empty or too long.",
    });

    await userEvent.type(screen.getByPlaceholderText("Search your dictionary"), "supa");
    expect(screen.getByText("Supabase")).toBeInTheDocument();
    expect(screen.queryByText("Figma")).not.toBeInTheDocument();
  });
});
