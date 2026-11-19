import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { LanguageRoutes_Serialize as LanguageRoutes, RoutePreview_Serialize as RoutePreview } from "@/lib/bindings";
import { useProviders } from "../providers";
import type { Settings } from "../store";
import { fakeHub } from "../testing";
import { Languages } from "./Languages";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const speech: RoutePreview = {
  label: "Auto → EN",
  model: "whisper-small",
  translation: "speech",
  sarvam: [],
  cloud: null,
  service: null,
  translator: null,
  problem: null,
};

type Answer = (cmd: string, args: Record<string, unknown>) => unknown;

function page(change?: (s: Settings) => void, dictate: RoutePreview = speech, answer?: Answer) {
  const saved = fakeHub(
    (s) => {
      s.onboarded = true;
      s.languages.speak = [
        { code: "en", engine: "local" },
        { code: "hi", engine: "local" },
      ];
      s.languages.output = "en";
      change?.(s);
    },
    (cmd, args) =>
      answer?.(cmd, args) ??
      (cmd === "language_routes" ? ({ dictate, alternate: null } satisfies LanguageRoutes) : undefined),
  );
  render(<Languages />);
  return saved;
}

/** Picks Sarvam for the second spoken language. */
async function sendHindiToSarvam() {
  await screen.findByText("Works well on this PC.");
  const [, hindi] = screen.getAllByRole("radiogroup", { name: /Where .* is turned into text/ });
  await userEvent.click(within(hindi).getByRole("radio", { name: /Sarvam/ }));
}

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
});

describe("Languages", () => {
  it("says how well each language works and how the text is made", async () => {
    page();
    expect(await screen.findByText("Works well on this PC.")).toBeInTheDocument();
    expect(screen.getByText(/Works on this PC, with some mistakes/)).toBeInTheDocument();
    expect(await screen.findByText("whisper-small turns your speech into English, right on this PC.")).toBeInTheDocument();
  });

  it("warns when the text has to be translated afterwards", async () => {
    page((s) => void (s.languages.output = "ta"), { ...speech, label: "Auto → TA", translation: "text" });
    expect(await screen.findByText(/then translates it into Tamil. This needs Sarvam or an AI model/)).toBeInTheDocument();
  });

  it("says who translates when Sarvam can", async () => {
    page((s) => void (s.languages.output = "ta"), {
      ...speech,
      label: "Auto → TA",
      translation: "text",
      translator: "Sarvam",
    });
    expect(await screen.findByText("Joona writes down what you say, then Sarvam translates it into Tamil.")).toBeInTheDocument();
  });

  it("says when Sarvam hears everything", async () => {
    page(undefined, { ...speech, model: null, sarvam: ["en", "hi"], cloud: "Sarvam" });
    expect(await screen.findByText("Sarvam turns your speech into English.")).toBeInTheDocument();
  });

  it("shows why dictation can't work yet", async () => {
    page(undefined, { ...speech, model: null, translation: "none", problem: "No speech model is installed yet." });
    expect(await screen.findByText("No speech model is installed yet.")).toBeInTheDocument();
  });

  it("adds and removes languages", async () => {
    const saved = page((s) => void (s.languages.current = "hi"));
    await userEvent.click(await screen.findByRole("button", { name: "Add a language" }));
    await userEvent.type(screen.getByPlaceholderText("Search languages"), "odia");
    await userEvent.click(screen.getByRole("button", { name: /Odia/ }));
    await vi.waitFor(() => expect(saved.at(-1)?.languages.speak.at(-1)).toEqual({ code: "or", engine: "local" }));
    expect(await screen.findByText("Models on this PC don't understand Odia. Choose Sarvam to use it.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Remove Hindi" }));
    await vi.waitFor(() => expect(saved.at(-1)?.languages.speak.map((l) => l.code)).toEqual(["en", "or"]));
    expect(saved.at(-1)?.languages.current).toBeNull();
  });

  it("asks before sending a language to Sarvam, and checks the key", async () => {
    const calls: string[] = [];
    const saved = page(undefined, speech, (cmd, args) => {
      if (cmd === "provider_test" || cmd === "provider_key_set") {
        calls.push(`${cmd} ${String(args.key)}`);
        return null;
      }
      if (cmd === "provider_keys") return calls.length > 1 ? ["sarvam"] : [];
      return undefined;
    });
    await sendHindiToSarvam();
    const sheet = await screen.findByRole("dialog", { name: "Turn on Sarvam" });
    expect(within(sheet).getByText(/Recordings in the languages you send to Sarvam/)).toBeInTheDocument();
    expect(saved).toHaveLength(0);

    await userEvent.type(within(sheet).getByLabelText("API key"), " sk_live_123 ");
    await userEvent.click(within(sheet).getByRole("button", { name: "Turn on Sarvam" }));
    await vi.waitFor(() => expect(saved.at(-1)?.languages.speak[1]).toEqual({ code: "hi", engine: "sarvam" }));
    expect(saved.at(-1)?.privacy.consented).toEqual(["sarvam"]);
    expect(calls).toEqual(["provider_test sk_live_123", "provider_key_set sk_live_123"]);
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("Sent to Sarvam AI to be turned into text.")).toBeInTheDocument();
  });

  it("doesn't save a key Sarvam refuses", async () => {
    const saved = page(undefined, speech, (cmd) => {
      if (cmd === "provider_test") throw { message: "Sarvam didn't accept your API key.", keyProblem: true };
      if (cmd === "provider_key_set") throw new Error("a refused key was saved");
      return undefined;
    });
    await sendHindiToSarvam();
    const sheet = await screen.findByRole("dialog", { name: "Turn on Sarvam" });
    await userEvent.type(within(sheet).getByLabelText("API key"), "wrong");
    await userEvent.click(within(sheet).getByRole("button", { name: "Turn on Sarvam" }));
    expect(await within(sheet).findByRole("alert")).toHaveTextContent("Sarvam didn't accept your API key.");
    expect(saved).toHaveLength(0);
  });

  it("sends a language straight to Sarvam once it's set up", async () => {
    const saved = page(
      (s) => void (s.privacy.consented = ["sarvam"]),
      speech,
      (cmd) => (cmd === "provider_keys" ? ["sarvam"] : undefined),
    );
    useProviders.setState({ keys: ["sarvam"] });
    await sendHindiToSarvam();
    await vi.waitFor(() => expect(saved.at(-1)?.languages.speak[1]).toEqual({ code: "hi", engine: "sarvam" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("offers the mixed script once a language goes to Sarvam", async () => {
    page((s) => {
      s.privacy.consented = ["sarvam"];
      s.languages.speak[1].engine = "sarvam";
      s.languages.script = "mixed";
    });
    expect(await screen.findByRole("radio", { name: "Mixed" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(/English words stay in English letters and the rest is in हिन्दी/)).toBeInTheDocument();
    expect(screen.getByText("Sent to Sarvam AI once you add your key on the Providers page.")).toBeInTheDocument();
  });

  it("keeps the last language", async () => {
    page((s) => void (s.languages.speak = [{ code: "en", engine: "local" }]));
    expect(await screen.findByRole("button", { name: "Remove English" })).toBeDisabled();
    expect(screen.queryByRole("combobox", { name: "Listen for" })).not.toBeInTheDocument();
  });

  it("turns the alternate shortcut off while the shortcut uses Shift", async () => {
    page((s) => void (s.shortcuts.trigger = { ctrl: true, win: false, alt: false, shift: true, key: null }));
    expect(await screen.findByText("Not available while your shortcut uses Shift.")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Alternate shortcut language" })).toBeDisabled();
  });
});
