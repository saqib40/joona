import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Heatmap } from "./Insights";

describe("Heatmap", () => {
  // A Wednesday.
  const today = new Date(2026, 10, 18);

  it("draws a year of weeks that end today", () => {
    const { container } = render(<Heatmap days={[]} today={today} />);
    const cells = container.querySelectorAll('[role="img"] > span');
    expect(cells).toHaveLength(53 * 7);
    // Thursday to Sunday of this week haven't happened yet.
    expect(container.querySelectorAll('[role="img"] > span.invisible')).toHaveLength(4);
  });

  it("shades busier days darker", () => {
    const days = [
      { date: "2026-11-16", words: 10, dictations: 1 },
      { date: "2026-11-17", words: 400, dictations: 12 },
      { date: "2026-11-18", words: 90, dictations: 4 },
    ];
    const { container } = render(<Heatmap days={days} today={today} />);
    const cell = (date: string) => container.querySelector(`[title^="${new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short" }).format(new Date(`${date}T00:00`))}"]`);
    expect(cell("2026-11-17")).toHaveClass("bg-accent");
    expect(cell("2026-11-16")).toHaveClass("bg-accent/30");
    expect(cell("2026-11-15")).toHaveClass("bg-fill-strong");
    expect(cell("2026-11-17")?.getAttribute("title")).toContain("400 words");
  });
});
