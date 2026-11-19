import { describe, expect, it } from "vitest";
import {
  dayKey,
  dayLabel,
  formatBytes,
  formatClip,
  formatMemory,
  formatMinutes,
  formatNumber,
  greeting,
  plural,
} from "./format";

const at = (day: number, hour = 12, minute = 0) => new Date(2026, 10, day, hour, minute).getTime();
const now = at(18, 15);

describe("dayLabel", () => {
  it("names recent days", () => {
    expect(dayLabel(at(18, 0, 5), now)).toBe("Today");
    expect(dayLabel(at(17, 23, 59), now)).toBe("Yesterday");
    expect(dayLabel(at(15), now)).toBe(new Intl.DateTimeFormat(undefined, { weekday: "long" }).format(at(15)));
  });

  it("gives the date for older days, with the year only when it differs", () => {
    const thisYear = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });
    expect(dayLabel(at(2), now)).toBe(thisYear.format(at(2)));
    const lastYear = new Date(2025, 11, 30).getTime();
    expect(dayLabel(lastYear, now)).toContain("2025");
  });

  it("keys days by local date", () => {
    expect(dayKey(at(8, 0, 30))).toBe("2026-11-08");
    expect(dayKey(at(8, 23, 59))).toBe("2026-11-08");
  });
});

describe("sizes and durations", () => {
  it("formats bytes", () => {
    expect(formatBytes(1_081_140_203)).toBe("1.1 GB");
    expect(formatBytes(487_601_967)).toBe("488 MB");
    expect(formatBytes(4271)).toBe("4 KB");
    expect(formatBytes(12)).toBe("12 B");
  });

  it("formats memory in megabytes", () => {
    expect(formatMemory(500)).toBe("500 MB");
    expect(formatMemory(16_384)).toBe("16.0 GB");
  });

  it("formats minutes and clips", () => {
    expect(formatMinutes(45)).toBe("45 min");
    expect(formatMinutes(120)).toBe("2 h");
    expect(formatMinutes(200)).toBe("3 h 20 min");
    expect(formatClip(7_000)).toBe("0:07");
    expect(formatClip(760_000)).toBe("12:40");
  });

  it("pluralises with grouped numbers", () => {
    expect(plural(1, "word")).toBe("1 word");
    expect(plural(0, "day")).toBe("0 days");
    expect(plural(1200, "dictation")).toBe(`${formatNumber(1200)} dictations`);
  });
});

describe("greeting", () => {
  it("follows the time of day", () => {
    expect(greeting(new Date(2026, 10, 18, 3), "Asha")).toBe("Working late, Asha");
    expect(greeting(new Date(2026, 10, 18, 9), "Asha")).toBe("Good morning, Asha");
    expect(greeting(new Date(2026, 10, 18, 14), null)).toBe("Good afternoon");
    expect(greeting(new Date(2026, 10, 18, 20), undefined)).toBe("Good evening");
  });
});
