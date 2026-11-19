import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { UpdateStatus_Serialize as UpdateStatus } from "@/lib/bindings";
import { Sidebar } from "../../Sidebar";
import { type Settings, useHub } from "../../store";
import { fakeHub } from "../../testing";
import { useUpdates } from "../../updates";
import { About } from "./About";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  useUpdates.setState({ status: null });
});

const AVAILABLE: UpdateStatus = {
  state: "available",
  version: "0.2.0",
  notes: "## Command Mode works in more apps\n\nAnd more.",
};

/** Shows About with Rust saying `status`, and finding `found` when asked to check. */
function about(status: UpdateStatus, found: UpdateStatus = status) {
  const calls: string[] = [];
  const saved = fakeHub(undefined, (cmd) => {
    calls.push(cmd);
    if (cmd === "update_status") return status;
    if (cmd === "update_check") return found;
    return undefined;
  });
  render(<About settings={useHub.getState().settings as Settings} />);
  return { calls, saved };
}

describe("updates", () => {
  it("offers a waiting update and shows it downloading", async () => {
    const user = userEvent.setup();
    const { calls } = about(AVAILABLE);
    expect(await screen.findByText("Joona 0.2.0 is available")).toBeInTheDocument();
    expect(screen.getByText("Command Mode works in more apps")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Update and restart" }));
    expect(calls).toContain("update_install");
    act(() => useUpdates.getState().receive({ state: "downloading", version: "0.2.0", percent: 40 }));
    expect(screen.getByText("Downloading Joona 0.2.0")).toBeInTheDocument();
    expect(screen.getByText("40%")).toBeInTheDocument();
  });

  it("checks when asked", async () => {
    const user = userEvent.setup();
    about({ state: "unknown" }, { state: "upToDate" });
    await user.click(await screen.findByRole("button", { name: "Check now" }));
    expect(await screen.findByText("You have the latest version.")).toBeInTheDocument();
  });

  it("says why it couldn't update, and tries again", async () => {
    const user = userEvent.setup();
    about({ state: "failed", message: "Can't reach GitHub. Check your internet connection." }, AVAILABLE);
    expect(await screen.findByText("Can't reach GitHub. Check your internet connection.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Joona 0.2.0 is available")).toBeInTheDocument();
  });

  it("can stop looking for updates", async () => {
    const user = userEvent.setup();
    const { saved } = about({ state: "upToDate" });
    await user.click(await screen.findByRole("switch", { name: "Check for updates automatically" }));
    expect(saved.at(-1)?.general.checkUpdates).toBe(false);
  });

  it("the sidebar points to a waiting update", async () => {
    const user = userEvent.setup();
    fakeHub();
    render(<Sidebar />);
    expect(screen.queryByText(/is available/)).not.toBeInTheDocument();

    act(() => useUpdates.getState().receive(AVAILABLE));
    await user.click(screen.getByRole("button", { name: "Joona 0.2.0 is available" }));
    expect(useHub.getState().page).toBe("settings");
    expect(useHub.getState().section).toBe("about");
  });
});
