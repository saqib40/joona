import { describe, expect, it } from "vitest";
import { downloadState, fraction, type Progress } from "./models";

const progress = (p: Partial<Progress>): Progress => ({
  id: "whisper-small",
  stage: "downloading",
  done: 0,
  total: 1000,
  bytesPerSec: 0,
  error: null,
  ...p,
});

describe("downloads", () => {
  it("says what a model's card shows", () => {
    expect(downloadState(undefined).kind).toBe("idle");
    expect(downloadState(progress({ stage: "done", done: 1000 })).kind).toBe("idle");
    expect(downloadState(progress({ stage: "cancelled", done: 0 })).kind).toBe("idle");
    expect(downloadState(progress({ stage: "cancelled", done: 400 })).kind).toBe("paused");
    expect(downloadState(progress({ stage: "failed", error: "offline" })).kind).toBe("failed");
    expect(downloadState(progress({ stage: "verifying" })).kind).toBe("running");
  });

  it("works out how far along a download is", () => {
    expect(fraction(progress({ done: 250 }))).toBe(0.25);
    expect(fraction(progress({ total: 0 }))).toBeNull();
    expect(fraction(progress({ stage: "converting", done: 600 }))).toBe(0.6);
    expect(fraction(progress({ stage: "verifying", done: 1000 }))).toBeNull();
  });
});
