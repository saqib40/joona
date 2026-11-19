import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AiOverview_Serialize as AiOverview, AiProvider_Serialize as AiProvider } from "@/lib/bindings";
import { privacySummary } from "../Sidebar";
import { isLocalUrl, sendsAway, useAi } from "../ai";
import { useModels } from "../models";
import { type Settings, useHub } from "../store";
import { fakeHub } from "../testing";
import { Providers } from "./Providers";

vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn(), warning: vi.fn() }) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const base = {
  key: "never",
  local: false,
  endpoint: false,
  defaultEndpoint: null,
  models: [],
  keysUrl: null,
  privacyUrl: null,
  note: null,
} satisfies Omit<AiProvider, "id" | "name" | "summary">;

const OPENAI: AiProvider = {
  ...base,
  id: "openai",
  name: "OpenAI",
  summary: "GPT models.",
  key: "required",
  defaultEndpoint: "https://api.openai.com/v1",
  models: ["gpt-4.1-mini", "gpt-4.1"],
  keysUrl: "https://platform.openai.com/api-keys",
  privacyUrl: "https://openai.com/policies/privacy-policy/",
};
const LOCAL: AiProvider = {
  ...base,
  id: "local",
  name: "Local AI",
  summary: "A free model on this PC.",
  local: true,
  models: ["qwen3-4b-instruct"],
};
const OLLAMA: AiProvider = {
  ...base,
  id: "ollama",
  name: "Ollama",
  summary: "Models you run yourself with Ollama.",
  local: true,
  endpoint: true,
  defaultEndpoint: "http://localhost:11434/v1",
};
const PROVIDERS = [OPENAI, LOCAL, OLLAMA];
const SAMPLE = { before: "um so it works", after: "So it works.", ms: 1200 };

/** A small copy of `ai::chosen`, from the settings Joona has now. */
function overview(keys: string[], installed: boolean): AiOverview {
  const s = useHub.getState().settings as Settings;
  const spec = PROVIDERS.find((p) => p.id === s.polish.provider);
  const cloud = spec ? sendsAway(spec, s.polish.endpoint) : false;
  const problem = !spec
    ? null
    : cloud && !s.privacy.consented.includes(`ai:${spec.id}`)
      ? `Joona doesn't have your permission to send text to ${spec.name}.`
      : spec.key === "required" && !keys.includes(spec.id)
        ? `Your ${spec.name} API key is missing.`
        : spec.id === "local" && !installed
          ? "The local AI model isn't downloaded."
          : null;
  const ready =
    spec && !problem
      ? { provider: spec.id, name: spec.name, model: s.polish.model ?? spec.models[0] ?? "", cloud }
      : null;
  return {
    providers: PROVIDERS,
    ready,
    problem,
    local: {
      name: "Qwen3 4B Instruct",
      size: 2_522_836_330,
      ramMb: 3400,
      license: "Apache-2.0",
      credit: "Qwen3 by the Qwen team",
      installed,
      running: false,
      download: null,
    },
  };
}

/** Fakes the AI commands on top of `fakeHub`. Returns the saved settings and the commands called. */
function fakeAi(change?: (s: Settings) => void, state = { keys: [] as string[], installed: false }) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  const saved = fakeHub(change, (cmd, args) => {
    if (cmd.startsWith("ai_") || cmd.startsWith("local_ai_") || cmd.startsWith("provider_key")) calls.push({ cmd, args });
    switch (cmd) {
      case "provider_keys":
        return state.keys;
      case "provider_test":
        return null;
      case "provider_key_set":
        state.keys = [...state.keys, args.provider as string];
        return null;
      case "provider_key_delete":
        state.keys = state.keys.filter((k) => k !== args.provider);
        return null;
      case "ai_overview":
        return overview(state.keys, state.installed);
      case "ai_models":
        return args.provider === "ollama" ? ["llama3.2:3b", "qwen2.5:7b"] : [];
      case "ai_test":
        return SAMPLE;
      default:
        return undefined;
    }
  });
  return { saved, calls, state };
}

beforeEach(() => {
  useAi.setState({ overview: null, sample: null, testing: false, error: null });
  useModels.setState({ downloads: {} });
});

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
});

describe("AI models", () => {
  it("asks before text goes to a cloud model, then tries it", async () => {
    const { saved, calls } = fakeAi();
    render(<Providers />);
    expect(await screen.findByText("No AI model yet")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add key…" }));
    const sheet = await screen.findByRole("dialog", { name: "Use OpenAI" });
    expect(within(sheet).getByText("What OpenAI receives")).toBeInTheDocument();
    expect(within(sheet).getByLabelText("Model")).toHaveValue("gpt-4.1-mini");
    const use = within(sheet).getByRole("button", { name: "Use OpenAI" });
    expect(use).toBeDisabled();

    await userEvent.type(within(sheet).getByLabelText("API key"), "sk-test-key");
    await userEvent.click(use);
    await vi.waitFor(() => expect(calls.map((c) => c.cmd)).toContain("ai_test"));
    const last = saved.at(-1);
    expect(last?.polish).toMatchObject({ provider: "openai", model: null, level: "smart" });
    expect(last?.privacy.consented).toEqual(["ai:openai"]);
    expect(calls.find((c) => c.cmd === "provider_key_set")?.args).toMatchObject({ provider: "openai", key: "sk-test-key" });

    expect(await screen.findByText("So it works.")).toBeInTheDocument();
    expect(screen.getByText("Cloud")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Smart polish" })).toBeChecked();
  });

  it("needs no permission for a server on this PC, and lists its models", async () => {
    const { saved } = fakeAi();
    render(<Providers />);
    await userEvent.click(await screen.findByRole("button", { name: "Use…" }));
    const sheet = await screen.findByRole("dialog", { name: "Use Ollama" });
    await vi.waitFor(() => expect(within(sheet).getByLabelText("Model")).toHaveValue("llama3.2:3b"));
    expect(within(sheet).queryByText("What Ollama receives")).not.toBeInTheDocument();
    expect(within(sheet).queryByLabelText("API key")).not.toBeInTheDocument();

    // An address on another PC means the text leaves this one.
    const address = within(sheet).getByLabelText("Server address");
    await userEvent.type(address, "http://192.168.1.20:11434");
    expect(within(sheet).getByText("What Ollama receives")).toBeInTheDocument();
    await userEvent.clear(address);
    await userEvent.type(address, "localhost:11434");
    expect(within(sheet).queryByText("What Ollama receives")).not.toBeInTheDocument();

    await userEvent.click(within(sheet).getByRole("button", { name: "Use Ollama" }));
    await vi.waitFor(() =>
      expect(saved.at(-1)?.polish).toMatchObject({
        provider: "ollama",
        model: "llama3.2:3b",
        endpoint: "localhost:11434",
      }),
    );
    expect(saved.at(-1)?.privacy.consented).toEqual([]);
  });

  it("downloads Joona's own model, then uses it", async () => {
    const { saved, calls, state } = fakeAi();
    render(<Providers />);
    await userEvent.click(await screen.findByRole("button", { name: "Download" }));
    expect(calls.map((c) => c.cmd)).toContain("local_ai_download");

    act(() =>
      useModels.getState().progress({
        id: "local-ai",
        stage: "downloading",
        done: 1_000_000_000,
        total: 2_522_836_330,
        bytesPerSec: 0,
        error: null,
      }),
    );
    expect(await screen.findByText(/Downloading 39%/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    state.installed = true;
    await act(async () => {
      useModels.getState().progress({ id: "local-ai", stage: "done", done: 0, total: 0, bytesPerSec: 0, error: null });
      await useAi.getState().refresh();
    });
    await userEvent.click(await screen.findByRole("button", { name: "Use" }));
    await vi.waitFor(() => expect(saved.at(-1)?.polish).toMatchObject({ provider: "local", level: "smart" }));
    expect(await screen.findByText("On this PC")).toBeInTheDocument();
    expect(saved.at(-1)?.privacy.consented).toEqual([]);
  });

  it("stopping goes back to Basic polish", async () => {
    const { saved } = fakeAi(
      (s) => {
        s.polish.provider = "openai";
        s.polish.level = "smart";
        s.privacy.consented = ["ai:openai"];
      },
      { keys: ["openai"], installed: false },
    );
    render(<Providers />);
    await userEvent.click(await screen.findByRole("button", { name: "Stop using" }));
    await vi.waitFor(() => expect(saved.at(-1)?.polish).toMatchObject({ provider: null, level: "basic" }));
    expect(await screen.findByText("No AI model yet")).toBeInTheDocument();
  });

  it("says what's missing", async () => {
    fakeAi((s) => {
      s.polish.provider = "openai";
      s.privacy.consented = ["ai:openai"];
    });
    render(<Providers />);
    expect(await screen.findByText("OpenAI isn't ready")).toBeInTheDocument();
    expect(screen.getByText(/Your OpenAI API key is missing/)).toBeInTheDocument();
  });
});

describe("where text goes", () => {
  it("knows which addresses are on this PC", () => {
    expect(isLocalUrl("http://localhost:11434/v1")).toBe(true);
    expect(isLocalUrl("127.0.0.1:8080")).toBe(true);
    expect(isLocalUrl("http://[::1]:8080")).toBe(true);
    expect(isLocalUrl("https://api.openai.com/v1")).toBe(false);
    expect(isLocalUrl("http://192.168.1.20:11434")).toBe(false);
    expect(sendsAway(OLLAMA, null)).toBe(false);
    expect(sendsAway(OLLAMA, "http://gpu-box:11434")).toBe(true);
    expect(sendsAway(LOCAL, null)).toBe(false);
    expect(sendsAway(OPENAI, null)).toBe(true);
  });

  it("the sidebar says cloud only when Smart polish sends text away", () => {
    fakeHub((s) => void (s.polish.level = "smart"));
    const settings = useHub.getState().settings;
    const local = { provider: "local", name: "Local AI", model: "qwen3-4b-instruct", cloud: false };
    const openai = { provider: "openai", name: "OpenAI", model: "gpt-4.1-mini", cloud: true };
    expect(privacySummary(settings, null).cloud).toBe(false);
    expect(privacySummary(settings, local).cloud).toBe(false);
    expect(privacySummary(settings, openai)).toEqual({ cloud: true, text: "Cloud: Smart polish with OpenAI" });
  });
});
