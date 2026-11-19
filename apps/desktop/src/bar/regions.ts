import { useEffect } from "react";
import { bar } from "./connect";

/** Extra room around each region, so the cursor can travel between them. */
const PAD = 6;
const EVERY_MS = 80;

/**
 * Tells Rust which parts of the window take the mouse: every element marked
 * `data-hit`. The rest of the window is click-through.
 */
export function useHitRegions() {
  useEffect(() => {
    let last = "";
    const report = () => {
      const regions = Array.from(document.querySelectorAll<HTMLElement>("[data-hit]"))
        .map((el) => el.getBoundingClientRect())
        .filter((r) => r.width > 0 && r.height > 0)
        .map((r) => ({
          x: Math.floor(r.left) - PAD,
          y: Math.floor(r.top) - PAD,
          width: Math.ceil(r.width) + PAD * 2,
          height: Math.ceil(r.height) + PAD * 2,
        }));
      const key = JSON.stringify(regions);
      if (key !== last) {
        last = key;
        void bar.setRegions(regions);
      }
    };
    report();
    const id = window.setInterval(report, EVERY_MS);
    return () => window.clearInterval(id);
  }, []);
}
