import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SttOverview_Serialize as SttOverview, SttProvider_Serialize as SttProvider } from "@/lib/bindings";
import { privacySummary } from "../Sidebar";
import { type Settings, useHub } from "../store";
import { useStt } from "../stt";
import { fakeHub } from "../testing";
import { Providers } from "./Providers";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
  useStt.setState({ overview: null });
});

const DEEPGRAM: SttProvider = {
  id: "deepgram",
  name: "Deepgram",
  summary: "Nova-3.",
  key: "required",
  endpoint: false,
  models: ["nova-3", "nova-2"],
  translates: [],
  terms: true,
  keysUrl: "https://console.deepgram.com/",
  privacyUrl: "https://deepgram.com/privacy",
  note: null,
};

const OWN: SttProvider = {
  ...DEEPGRAM,
  id: "custom-stt",
  name: "Your own server",
  summary: "A Whisper server.",
  key: "optional",
  endpoint: true,
  models: [],
  keysUrl: null,
  privacyUrl: null,
};

/** Rust's answer for `settings`, as `ipc::stt_overview` works it out. */
function overview(settings: Settings, keys: string[]): SttOverview {
  const { provider, model, endpoint } = settings.transcription;
  const spec = [DEEPGRAM, OWN].find((p) => p.id === provider);
  if (!spec) return { providers: [DEEPGRAM, OWN], ready: null, problem: null };
  const cloud = !spec.endpoint;
  const problem =
    cloud && !settings.privacy.consented.includes(`stt:${spec.id}`)
      ? `Joona doesn't have your permission to send recordings to ${spec.name}.`
      : spec.key === "required" && !keys.includes(spec.id)
        ? `Your ${spec.name} API key is missing.`
        : null;
  const ready =
    problem || (spec.endpoint && !endpoint)
      ? null
      : { provider: spec.id, name: spec.name, model: model ?? spec.models[0] ?? "", cloud, translates: false };
  return { providers: [DEEPGRAM, OWN], ready, problem };
}

function hub(change?: (s: Settings) => void, startKeys: string[] = []) {
  let keys = startKeys;
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  const saved: Settings[] = fakeHub(change, (cmd, args) => {
    calls.push({ cmd, args });
    const latest = saved.at(-1) ?? (useHub.getState().settings as Settings);
    switch (cmd) {
      case "provider_keys":
        return keys;
      case "provider_key_set":
        keys = [...keys, args.provider as string];
        return null;
      case "provider_key_delete":
        keys = keys.filter((k) => k !== args.provider);
        return null;
      case "provider_test":
        return null;
      case "stt_overview":
        return overview(latest, keys);
      default:
        return undefined;
    }
  });
  return { saved, calls };
}

describe("Speech to text", () => {
  it("uses a cloud service once the user agrees and adds a key", async () => {
    const { saved, calls } = hub();
    render(<Providers />);
    expect(await screen.findByRole("heading", { name: "Speech to text" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add key…" }));
    const sheet = await screen.findByRole("dialog", { name: "Use Deepgram" });
    expect(within(sheet).getByText("What Deepgram receives")).toBeInTheDocument();
    expect(within(sheet).getByText(/The words in your dictionary/)).toBeInTheDocument();
    await userEvent.type(within(sheet).getByLabelText("API key"), "dg-test-key");
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Deepgram" }));

    await vi.waitFor(() => expect(saved.at(-1)?.transcription.provider).toBe("deepgram"));
    expect(saved.at(-1)?.transcription.model).toBeNull();
    expect(saved.at(-1)?.privacy.consented).toEqual(["stt:deepgram"]);
    expect(calls.find((c) => c.cmd === "provider_key_set")?.args.provider).toBe("deepgram");
    expect(toast.success).toHaveBeenCalledWith("Deepgram hears your dictations now", {
      description: "The bar shows a cloud badge while it listens.",
    });
    expect(await screen.findByText("Deepgram hears your dictations")).toBeInTheDocument();
  });

  it("a server on this PC needs no permission or key", async () => {
    const { saved, calls } = hub();
    render(<Providers />);
    await userEvent.click(await screen.findByRole("button", { name: "Use…" }));
    const sheet = await screen.findByRole("dialog", { name: "Use Your own server" });
    const use = within(sheet).getByRole("button", { name: "Use Your own server" });
    expect(use).toBeDisabled();

    await userEvent.type(within(sheet).getByLabelText("Server address"), "localhost:8000/v1");
    expect(within(sheet).getByText("This server is on this PC, so your recordings stay here.")).toBeInTheDocument();
    expect(within(sheet).queryByText(/receives/)).not.toBeInTheDocument();
    await userEvent.type(within(sheet).getByLabelText("Model"), "whisper-1");
    await userEvent.click(use);

    await vi.waitFor(() => expect(saved.at(-1)?.transcription.provider).toBe("custom-stt"));
    expect(saved.at(-1)?.transcription).toEqual({
      provider: "custom-stt",
      model: "whisper-1",
      endpoint: "localhost:8000/v1",
    });
    expect(saved.at(-1)?.privacy.consented).toEqual([]);
    expect(calls.find((c) => c.cmd === "provider_test")?.args).toMatchObject({
      provider: "custom-stt",
      key: null,
      endpoint: "localhost:8000/v1",
    });
  });

  it("deleting the key stops sending recordings", async () => {
    const { saved } = hub(
      (s) => {
        s.transcription.provider = "deepgram";
        s.privacy.consented = ["stt:deepgram"];
      },
      ["deepgram"],
    );
    render(<Providers />);
    expect(await screen.findByText("Deepgram hears your dictations")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Delete your Deepgram key" }));
    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(saved.at(-1)?.transcription.provider).toBeNull());
    expect(saved.at(-1)?.privacy.consented).toEqual([]);
  });

  it("says it's not ready until the key is there", async () => {
    hub((s) => {
      s.transcription.provider = "deepgram";
      s.privacy.consented = ["stt:deepgram"];
    });
    render(<Providers />);
    expect(await screen.findByText("Deepgram isn't ready")).toBeInTheDocument();
    expect(screen.getByText(/Your Deepgram API key is missing\. Until it is/)).toBeInTheDocument();
  });

  it("the sidebar says when recordings go to the cloud", () => {
    hub();
    const settings = useHub.getState().settings;
    const deepgram = { provider: "deepgram", name: "Deepgram", model: "nova-3", cloud: true, translates: false };
    expect(privacySummary(settings, null, deepgram)).toEqual({
      cloud: true,
      text: "Cloud: Deepgram hears your dictations",
    });
    expect(privacySummary(settings, null, { ...deepgram, cloud: false }).cloud).toBe(false);
  });
});
