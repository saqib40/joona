import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { toast } from "sonner";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeHub } from "../testing";
import { Providers } from "./Providers";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn(), warning: vi.fn() } }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve));
  clearMocks();
  vi.clearAllMocks();
});

describe("Providers", () => {
  it("turns Sarvam on with a key it accepts", async () => {
    let keys: string[] = [];
    const saved = fakeHub(undefined, (cmd, args) => {
      if (cmd === "provider_keys") return keys;
      if (cmd === "provider_test") return null;
      if (cmd === "provider_key_set") {
        keys = [args.provider as string];
        return null;
      }
      return undefined;
    });
    render(<Providers />);
    expect(await screen.findByText("Off")).toBeInTheDocument();
    expect(screen.getByText("Not added yet.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Turn on Sarvam" }));
    const sheet = await screen.findByRole("dialog", { name: "Turn on Sarvam" });
    await userEvent.type(within(sheet).getByLabelText("API key"), "sk_live");
    await userEvent.click(within(sheet).getByRole("button", { name: "Turn on Sarvam" }));
    await vi.waitFor(() => expect(saved.at(-1)?.privacy.consented).toEqual(["sarvam"]));
    expect(await screen.findByText("On")).toBeInTheDocument();
    expect(screen.getByText("Saved in Windows Credential Manager.")).toBeInTheDocument();
  });

  it("tests the key, and turning Sarvam off deletes it", async () => {
    const calls: string[] = [];
    const saved = fakeHub(
      (s) => {
        s.privacy.consented = ["sarvam"];
        s.languages.speak = [{ code: "hi", engine: "sarvam" }];
      },
      (cmd) => {
        if (cmd === "provider_keys") return calls.includes("provider_key_delete") ? [] : ["sarvam"];
        if (cmd === "provider_test" || cmd === "provider_key_delete") {
          calls.push(cmd);
          return null;
        }
        return undefined;
      },
    );
    render(<Providers />);
    expect(await screen.findByText("On")).toBeInTheDocument();
    expect(screen.getByText(/Turning Hindi into text/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Test" }));
    await vi.waitFor(() => expect(toast.success).toHaveBeenCalledWith("Sarvam accepted your key"));

    await userEvent.click(screen.getByRole("button", { name: "Turn off…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Turn off" }));
    await vi.waitFor(() => expect(saved.at(-1)?.privacy.consented).toEqual([]));
    expect(calls).toEqual(["provider_test", "provider_key_delete"]);
    expect(await screen.findByText("Off")).toBeInTheDocument();
  });

  it("says why a saved key stopped working", async () => {
    fakeHub(
      (s) => void (s.privacy.consented = ["sarvam"]),
      (cmd) => {
        if (cmd === "provider_keys") return ["sarvam"];
        if (cmd === "provider_test") throw { message: "Your Sarvam account is out of credits.", keyProblem: false };
        return undefined;
      },
    );
    render(<Providers />);
    await userEvent.click(await screen.findByRole("button", { name: "Test" }));
    await vi.waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Sarvam didn't work", {
        description: "Your Sarvam account is out of credits.",
      }),
    );
  });
});
