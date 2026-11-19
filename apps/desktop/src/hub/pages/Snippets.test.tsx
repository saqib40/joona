import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { Snippet_Serialize as Snippet } from "@/lib/bindings";
import { fakeHub } from "../testing";
import { Snippets } from "./Snippets";

vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }) }));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
});

function snippet(id: number, phrase: string, text: string): Snippet {
  return { id, phrase, text, uses: 0, lastUsed: null, createdAt: 0 };
}

function fakeSnippets(start: Snippet[]) {
  let snippets = start;
  const calls: [string, Record<string, unknown>][] = [];
  fakeHub(undefined, (cmd, args) => {
    if (cmd === "snippets_list") return snippets;
    if (cmd === "snippet_add") {
      calls.push([cmd, args]);
      const added = snippet(10, args.phrase as string, args.text as string);
      snippets = [...snippets, added];
      return added;
    }
    if (cmd === "snippet_delete") {
      calls.push([cmd, args]);
      snippets = snippets.filter((s) => s.id !== args.id);
      return null;
    }
    return undefined;
  });
  return calls;
}

const show = () => render(<Snippets />, { wrapper: TooltipProvider });

describe("Snippets", () => {
  it("adds a snippet with several lines", async () => {
    const calls = fakeSnippets([]);
    show();
    expect(await screen.findByText("Say less, type more")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add snippet" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a snippet" });
    await userEvent.type(within(dialog).getByLabelText("When I say"), "my address");
    await userEvent.type(within(dialog).getByLabelText("Joona types"), "Flat 4B{Enter}12 MG Road");
    expect(within(dialog).getByText(/^18 \//)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Add" }));

    expect(await screen.findByText("“my address”")).toBeInTheDocument();
    expect(calls).toEqual([["snippet_add", { phrase: "my address", text: "Flat 4B\n12 MG Road" }]]);
  });

  it("won't save text over the limit", async () => {
    fakeSnippets([]);
    show();
    await userEvent.click(await screen.findByRole("button", { name: "Add snippet" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a snippet" });
    await userEvent.type(within(dialog).getByLabelText("When I say"), "lorem");
    await userEvent.click(within(dialog).getByLabelText("Joona types"));
    await userEvent.paste("x".repeat(4_001));
    expect(within(dialog).getByRole("button", { name: "Add" })).toBeDisabled();
  });

  it("asks before deleting", async () => {
    const calls = fakeSnippets([snippet(1, "sign off", "Thanks,\nAsha")]);
    show();
    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    const confirm = await screen.findByRole("alertdialog", { name: "Delete this snippet?" });
    await userEvent.click(within(confirm).getByRole("button", { name: "Delete" }));

    expect(await screen.findByText("Say less, type more")).toBeInTheDocument();
    expect(calls).toEqual([["snippet_delete", { id: 1 }]]);
  });
});
