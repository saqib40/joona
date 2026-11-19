import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { events } from "@/lib/bindings";
import { useProviders } from "../providers";
import { type Settings, useHub } from "../store";
import { fakeHub } from "../testing";
import { Boosts } from "./Boosts";
import { Done } from "./Done";
import { LanguagesStep } from "./LanguagesStep";
import { Practice } from "./Practice";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

let saved: Settings[] = [];

/** The Hub with default settings and `change` applied. Returns a step's props. */
function hub(change?: (s: Settings) => void, answer?: Parameters<typeof fakeHub>[1]) {
  saved = fakeHub(change, answer);
  return { next: vi.fn(), back: vi.fn() };
}

afterEach(async () => {
  cleanup();
  // Event listeners are removed once their `listen` promises settle, which needs the mocks.
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
});

describe("LanguagesStep", () => {
  it("saves what you speak and keeps each language's engine", async () => {
    const step = hub((s) => {
      s.languages.speak = [
        { code: "en", engine: "local" },
        { code: "ta", engine: "sarvam" },
      ];
      s.languages.current = "en";
    });
    render(<LanguagesStep {...step} />);
    await userEvent.click(await screen.findByRole("button", { name: /Hindi/ }));
    await userEvent.click(screen.getByRole("button", { name: "English" }));
    expect(screen.getByText("Tamil, Hindi")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await vi.waitFor(() => expect(step.next).toHaveBeenCalled());
    expect(saved.at(-1)?.languages).toMatchObject({
      speak: [
        { code: "ta", engine: "sarvam" },
        { code: "hi", engine: "local" },
      ],
      output: null,
      current: null,
    });
  });

  it("doesn't translate English into English", async () => {
    const step = hub((s) => {
      s.languages.speak = [
        { code: "en", engine: "local" },
        { code: "hi", engine: "local" },
      ];
      s.languages.output = "en";
    });
    render(<LanguagesStep {...step} />);
    expect(await screen.findByText("English, Hindi → English")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Hindi/ }));
    expect(screen.queryByText(/→/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await vi.waitFor(() => expect(step.next).toHaveBeenCalled());
    expect(saved.at(-1)?.languages.output).toBeNull();
  });

  it("needs at least one language", async () => {
    const step = hub();
    render(<LanguagesStep {...step} />);
    await userEvent.click(await screen.findByRole("button", { name: "English" }));
    expect(screen.getByText("Pick at least one")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
  });

  it("finds languages that aren't shown by their own name", async () => {
    hub();
    render(<LanguagesStep next={vi.fn()} back={vi.fn()} />);
    expect(await screen.findByText(/Search for 1 more/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /French/ })).not.toBeInTheDocument();

    await userEvent.type(screen.getByPlaceholderText("Search languages"), "franç");
    await userEvent.click(screen.getByRole("button", { name: /French/ }));
    await userEvent.clear(screen.getByPlaceholderText("Search languages"));
    expect(screen.getByRole("button", { name: /French/ })).toHaveAttribute("aria-pressed", "true");
  });
});

describe("Practice", () => {
  const route = { text: "EN", crossLanguage: false, cloud: null };

  it("follows the dictation and counts the words that arrive", async () => {
    render(<Practice {...hub()} />);
    expect(screen.getByRole("button", { name: "Skip" })).toBeInTheDocument();

    await act(() =>
      events.barStateChanged.emit({
        state: "listening",
        mode: "dictate",
        handsFree: false,
        elapsedMs: 0,
        route,
        endingSoon: false,
      }),
    );
    expect(screen.getByText(/Listening/)).toBeInTheDocument();
    await act(() => events.barStateChanged.emit({ state: "processing", route }));
    expect(screen.getByText("Writing…")).toBeInTheDocument();

    fireEvent.input(screen.getByRole("textbox", { name: "Practice box" }), {
      target: { value: "Hi, I'm trying Joona " },
    });
    await act(() => events.barStateChanged.emit({ state: "done", outcome: { kind: "pasted" } }));
    expect(screen.getByText("Typed")).toBeInTheDocument();
    expect(screen.getByText("That's it. You dictated 4 words.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("says what went wrong", async () => {
    render(<Practice {...hub()} />);
    await act(() =>
      events.toastShown.emit({
        id: 1,
        tone: "error",
        title: "No speech model yet",
        detail: "Download one to start dictating.",
        actions: [],
        timeoutMs: 6000,
      }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("No speech model yet. Download one to start dictating.");
  });
});

describe("Boosts", () => {
  it("offers Sarvam for the Indian languages you speak, once you agree", async () => {
    hub(
      (s) => {
        s.languages.speak = [
          { code: "en", engine: "local" },
          { code: "or", engine: "local" },
          { code: "fr", engine: "local" },
        ];
      },
      (cmd) => (cmd === "provider_test" || cmd === "provider_key_set" ? null : undefined),
    );
    render(<Boosts next={vi.fn()} back={vi.fn()} />);
    const odia = await screen.findByRole("button", { name: /Odia/ });
    expect(screen.queryByRole("button", { name: /English|French/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Odia needs Sarvam/)).toBeInTheDocument();

    await userEvent.click(odia);
    const sheet = await screen.findByRole("dialog", { name: "Turn on Sarvam" });
    await userEvent.type(within(sheet).getByLabelText("API key"), "sk_test");
    await userEvent.click(within(sheet).getByRole("button", { name: "Turn on Sarvam" }));
    await vi.waitFor(() => expect(saved.at(-1)?.languages.speak[1]).toEqual({ code: "or", engine: "sarvam" }));
    expect(saved.at(-1)?.privacy.consented).toEqual(["sarvam"]);
    expect(screen.getByText(/Recordings in Odia are sent to Sarvam AI/)).toBeInTheDocument();
  });

  it("sends nothing to Sarvam when you cancel", async () => {
    hub((s) => {
      s.languages.speak = [{ code: "hi", engine: "local" }];
    });
    render(<Boosts next={vi.fn()} back={vi.fn()} />);
    await userEvent.click(await screen.findByRole("button", { name: /Hindi/ }));
    const sheet = await screen.findByRole("dialog", { name: "Turn on Sarvam" });
    await userEvent.click(within(sheet).getByRole("button", { name: "Cancel" }));
    await vi.waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(saved).toHaveLength(0);
    expect(screen.getByRole("button", { name: /Hindi/ })).toHaveAttribute("aria-pressed", "false");
  });

  it("leaves Sarvam out when it wouldn't help", async () => {
    hub();
    render(<Boosts next={vi.fn()} back={vi.fn()} />);
    expect(await screen.findByRole("switch", { name: "Smart polish" })).toBeInTheDocument();
    expect(screen.queryByText("Sarvam for Indian languages")).not.toBeInTheDocument();
  });
});

describe("Done", () => {
  it("goes to Providers when Smart polish needs a model", async () => {
    render(<Done {...hub((s) => void (s.polish.level = "smart"))} />);
    await userEvent.click(screen.getByRole("button", { name: "Pick an AI model" }));
    await vi.waitFor(() => expect(useHub.getState().page).toBe("providers"));
    expect(saved.at(-1)?.onboarded).toBe(true);
  });

  it("goes home otherwise", async () => {
    render(<Done {...hub()} />);
    await userEvent.click(screen.getByRole("button", { name: "Start using Joona" }));
    await vi.waitFor(() => expect(useHub.getState().page).toBe("home"));
    expect(saved.at(-1)?.onboarded).toBe(true);
  });

  it("doesn't ask for a Sarvam key that's already saved", async () => {
    const step = hub((s) => {
      s.languages.speak = [{ code: "hi", engine: "sarvam" }];
      s.privacy.consented = ["sarvam"];
    });
    useProviders.setState({ keys: ["sarvam"] });
    render(<Done {...step} />);
    expect(screen.getByRole("button", { name: "Start using Joona" })).toBeInTheDocument();
  });
});
