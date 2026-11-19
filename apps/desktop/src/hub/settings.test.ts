import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "./demo";
import { receiveSettings, saveSettings } from "./settings";
import { type Settings, useHub } from "./store";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

/** A `set_settings` that answers when the test says so. */
function pendingSaves() {
  const waiting: { settings: Settings; answer: (result: Settings | Error) => void }[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "set_settings") return null;
    const settings = (args as { settings: Settings }).settings;
    return new Promise((resolve, reject) =>
      waiting.push({ settings, answer: (r) => (r instanceof Error ? reject(r.message) : resolve(r)) }),
    );
  });
  return waiting;
}

describe("saveSettings", () => {
  beforeEach(() => useHub.setState({ settings: defaultSettings(), saving: 0 }));
  afterEach(() => clearMocks());

  it("shows a change at once, then what Rust saved", async () => {
    const waiting = pendingSaves();
    const saved = saveSettings((s) => void (s.models.idleUnloadMinutes = 999));
    expect(useHub.getState().settings?.models.idleUnloadMinutes).toBe(999);
    await vi.waitFor(() => expect(waiting).toHaveLength(1));

    const clamped = structuredClone(waiting[0].settings);
    clamped.models.idleUnloadMinutes = 240;
    waiting[0].answer(clamped);
    await expect(saved).resolves.toBe(true);
    expect(useHub.getState().settings?.models.idleUnloadMinutes).toBe(240);
    expect(useHub.getState().saving).toBe(0);
  });

  it("puts the old settings back when Rust refuses", async () => {
    const { toast } = await import("sonner");
    const waiting = pendingSaves();
    const saved = saveSettings((s) => void (s.general.sounds = false));
    await vi.waitFor(() => expect(waiting).toHaveLength(1));
    waiting[0].answer(new Error("disk full"));
    await expect(saved).resolves.toBe(false);
    expect(useHub.getState().settings?.general.sounds).toBe(true);
    expect(toast.error).toHaveBeenCalledWith("Couldn't save that", { description: "disk full" });
  });

  it("ignores settings events while a save is on its way", async () => {
    const waiting = pendingSaves();
    const saved = saveSettings((s) => void (s.general.theme = "dark"));
    await vi.waitFor(() => expect(waiting).toHaveLength(1));

    // Rust's own event for this save, sent before the save returns.
    receiveSettings({ ...defaultSettings(), general: { ...defaultSettings().general, theme: "light" } });
    expect(useHub.getState().settings?.general.theme).toBe("dark");

    waiting[0].answer(waiting[0].settings);
    await saved;
    receiveSettings({ ...defaultSettings(), general: { ...defaultSettings().general, sounds: false } });
    expect(useHub.getState().settings?.general.sounds).toBe(false);
  });

  it("keeps quick changes in order", async () => {
    const waiting = pendingSaves();
    const first = saveSettings((s) => void (s.general.sounds = false));
    const second = saveSettings((s) => void (s.general.theme = "dark"));
    await vi.waitFor(() => expect(waiting).toHaveLength(2));
    expect(waiting[1].settings.general.sounds).toBe(false);

    waiting[0].answer(waiting[0].settings);
    await first;
    expect(useHub.getState().settings?.general.theme).toBe("dark");
    waiting[1].answer(waiting[1].settings);
    await second;
    expect(useHub.getState().settings?.general).toMatchObject({ sounds: false, theme: "dark" });
  });
});
