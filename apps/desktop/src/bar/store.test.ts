import { beforeEach, describe, expect, it } from "vitest";
import { useBar, type Toast } from "./store";

const toast = (id: number): Toast => ({
  id,
  tone: "neutral",
  title: `Toast ${id}`,
  detail: null,
  actions: [],
  timeoutMs: 4000,
});

describe("bar store", () => {
  beforeEach(() => {
    useBar.setState({ state: { state: "resting" }, toasts: [], hover: false, pillHover: false, menu: null });
  });

  it("keeps the newest two toasts", () => {
    const { addToast } = useBar.getState();
    [1, 2, 3].forEach((id) => addToast(toast(id)));
    expect(useBar.getState().toasts.map((t) => t.id)).toEqual([2, 3]);
  });

  it("replaces a toast shown again with the same id", () => {
    const { addToast } = useBar.getState();
    addToast(toast(1));
    addToast({ ...toast(1), title: "Again" });
    expect(useBar.getState().toasts).toHaveLength(1);
    expect(useBar.getState().toasts[0].title).toBe("Again");
  });

  it("forgets the pill hover when the cursor leaves the bar", () => {
    useBar.setState({ hover: true, pillHover: true });
    useBar.getState().setHover(false);
    expect(useBar.getState().pillHover).toBe(false);
  });

  it("starts the timer from the elapsed time and closes menus", () => {
    useBar.setState({ menu: "context" });
    const before = performance.now();
    useBar.getState().setState({
      state: "listening",
      mode: "dictate",
      handsFree: true,
      elapsedMs: 5000,
      route: { text: "EN", crossLanguage: false, cloud: null },
      endingSoon: false,
    });
    const { startedAt, menu } = useBar.getState();
    expect(menu).toBeNull();
    expect(before - startedAt).toBeGreaterThanOrEqual(4990);
  });
});
