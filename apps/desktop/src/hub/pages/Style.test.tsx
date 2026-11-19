import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppRule, Category, StyleOverview, StyleTone } from "@/lib/bindings";
import type { Settings } from "../store";
import { fakeHub } from "../testing";
import { Style } from "./Style";

vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }) }));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
});

const CATEGORIES: Category[] = ["personal", "work", "email", "other"];
const TONES: StyleTone[] = ["formal", "casual", "veryCasual", "excited"];

/** A fake `StyleMap` in Rust, and the changes made to it. */
function fakeStyles(change?: (s: Settings) => void) {
  const tones = new Map<Category, StyleTone>([
    ["personal", "casual"],
    ["work", "casual"],
    ["email", "formal"],
    ["other", "formal"],
  ]);
  const builtIn: AppRule[] = [
    { app: "whatsapp.exe", name: "WhatsApp", website: false, category: "personal", builtIn: true, changed: false },
    { app: "Gmail", name: "Gmail", website: true, category: "email", builtIn: true, changed: false },
  ];
  const chosen = new Map<string, Category>([["whatsapp.exe", "work"]]);
  const overview = (): StyleOverview => {
    const apps = builtIn.map((rule) => ({
      ...rule,
      category: chosen.get(rule.app) ?? rule.category,
      changed: chosen.has(rule.app),
    }));
    for (const [app, category] of chosen) {
      if (!apps.some((a) => a.app === app)) {
        apps.push({ app, name: app === "code.exe" ? "VS Code" : app, website: !app.endsWith(".exe"), category, builtIn: false, changed: true });
      }
    }
    return {
      categories: CATEGORIES.map((category) => ({
        category,
        tone: tones.get(category) ?? "formal",
        previews: TONES.map((tone) => ({ tone, text: `${category} as ${tone}` })),
      })),
      apps,
      recent: [{ app: "code.exe", name: "VS Code" }].filter((r) => !chosen.has(r.app)),
    };
  };
  const calls: [string, Record<string, unknown>][] = [];
  const saved = fakeHub(change, (cmd, args) => {
    if (cmd === "style_overview") return overview();
    if (cmd === "style_set_tone") {
      calls.push([cmd, args]);
      tones.set(args.category as Category, args.tone as StyleTone);
      return overview();
    }
    if (cmd === "style_set_app") {
      calls.push([cmd, args]);
      const app = args.app as string;
      if (!app.trim()) throw "Type the name of an app or website.";
      if (args.category) chosen.set(app, args.category as Category);
      else chosen.delete(app);
      return overview();
    }
    return undefined;
  });
  return { calls, saved };
}

describe("Style", () => {
  it("picks a tone for each kind of app", async () => {
    const { calls } = fakeStyles();
    render(<Style />);
    const tones = await screen.findByRole("radiogroup", { name: "Style for Personal messages" });
    expect(within(tones).getByRole("radio", { name: /^Casual/ })).toHaveAttribute("aria-checked", "true");
    expect(within(tones).getByText("personal as veryCasual")).toBeInTheDocument();

    await userEvent.click(within(tones).getByRole("radio", { name: /^Excited/ }));
    await vi.waitFor(() =>
      expect(within(tones).getByRole("radio", { name: /^Excited/ })).toHaveAttribute("aria-checked", "true"),
    );

    await userEvent.click(screen.getByRole("radio", { name: "Email" }));
    const email = screen.getByRole("radiogroup", { name: "Style for Email" });
    expect(within(email).getByRole("radio", { name: /^Formal/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Gmail")).toBeInTheDocument();
    expect(calls).toEqual([["style_set_tone", { category: "personal", tone: "excited" }]]);
  });

  it("lists apps under the category picked for them, and resets them", async () => {
    const { calls } = fakeStyles();
    render(<Style />);
    await screen.findByRole("radiogroup", { name: "Style for Personal messages" });
    expect(screen.queryByText("WhatsApp")).not.toBeInTheDocument();
    expect(screen.getByText("Nothing here yet")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Work messages" }));
    expect(screen.getByText("WhatsApp")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    await vi.waitFor(() => expect(screen.queryByText("WhatsApp")).not.toBeInTheDocument());
    expect(calls).toEqual([["style_set_app", { app: "whatsapp.exe", category: null }]]);
  });

  it("adds an app you've used, or a website", async () => {
    const { calls } = fakeStyles();
    render(<Style />);
    await userEvent.click(await screen.findByRole("button", { name: "Add" }));
    let dialog = await screen.findByRole("dialog", { name: "Add to Personal messages" });
    await userEvent.click(within(dialog).getByRole("button", { name: "VS Code" }));
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("VS Code")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    dialog = await screen.findByRole("dialog", { name: "Add to Personal messages" });
    expect(within(dialog).queryByRole("button", { name: "VS Code" })).not.toBeInTheDocument();
    await userEvent.type(within(dialog).getByLabelText("Website"), "Notion{Enter}");
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("Notion")).toBeInTheDocument();
    expect(calls).toEqual([
      ["style_set_app", { app: "code.exe", category: "personal" }],
      ["style_set_app", { app: "Notion", category: "personal" }],
    ]);
  });

  it("asks for polish when it's off", async () => {
    const { saved } = fakeStyles((s) => void (s.polish.level = "off"));
    render(<Style />);
    expect(await screen.findByText("Styles need polish")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Turn on Basic polish" }));
    await vi.waitFor(() => expect(saved.at(-1)?.polish.level).toBe("basic"));
  });
});
