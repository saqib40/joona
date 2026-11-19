import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { events, type Recording_Serialize as Recording, type Shortcut_Serialize as Shortcut } from "@/lib/bindings";
import { ShortcutField } from "./ShortcutField";

const keys = (s: Partial<Shortcut>): Shortcut => ({ ctrl: false, win: false, alt: false, shift: false, key: null, ...s });
const CTRL_WIN = keys({ ctrl: true, win: true });
const ALT_SHIFT_Z = keys({ alt: true, shift: true, key: 0x5a });

let calls: string[] = [];

function field(onChange = vi.fn()) {
  render(
    <ShortcutField
      label="the dictation shortcut"
      value={CTRL_WIN}
      taken={[{ shortcut: ALT_SHIFT_Z, name: "paste last dictation" }]}
      onChange={onChange}
    />,
  );
  return onChange;
}

/** Clicks the field and waits until Rust was asked to record. */
async function startRecording() {
  const before = calls.filter((c) => c === "shortcut_record").length;
  await userEvent.click(screen.getByRole("button", { name: "Change the dictation shortcut" }));
  await waitFor(() => expect(calls.filter((c) => c === "shortcut_record")).toHaveLength(before + 1));
}

const send = (recording: Recording) => act(() => events.shortcutRecording.emit(recording));
const done = (shortcut: Shortcut, label: string, trigger = true) => send({ kind: "done", shortcut, label, trigger });

describe("ShortcutField", () => {
  beforeEach(() => {
    calls = [];
    mockIPC((cmd) => void calls.push(cmd), { shouldMockEvents: true });
  });
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("shows the keys as they're pressed and saves the chord", async () => {
    const onChange = field();
    await startRecording();
    expect(screen.getByText("Press the keys…")).toBeInTheDocument();
    await send({ kind: "preview", shortcut: keys({ ctrl: true }), label: "Ctrl" });
    expect(screen.getByText("Ctrl")).toBeInTheDocument();

    const f8 = keys({ key: 0x77 });
    await done(f8, "F8");
    expect(onChange).toHaveBeenCalledWith(f8);
    expect(screen.queryByText(/Esc to cancel/)).not.toBeInTheDocument();
  });

  it("explains why a shortcut can't be used", async () => {
    const onChange = field();
    await startRecording();
    await done(keys({ key: 0x41 }), "A", false);
    expect(screen.getByRole("alert")).toHaveTextContent("A on its own would get in the way of typing");

    await startRecording();
    await done(keys({ win: true, key: 0x4c }), "Win+L");
    expect(screen.getByRole("alert")).toHaveTextContent("Win+L locks Windows, so pick another.");

    await startRecording();
    await done(ALT_SHIFT_Z, "Alt+Shift+Z");
    expect(screen.getByRole("alert")).toHaveTextContent("already the paste last dictation shortcut");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("stops recording when the Hub loses focus", async () => {
    const onChange = field();
    await startRecording();
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(calls).toContain("shortcut_record_cancel");
    await done(keys({ key: 0x77 }), "F8");
    expect(onChange).not.toHaveBeenCalled();
  });
});
