import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import { onLevel } from "./store";

/** Taller in the middle, like a voice. */
const WEIGHTS = [0.38, 0.52, 0.68, 0.82, 0.94, 1, 0.94, 0.82, 0.68, 0.52, 0.38];
const MIN = 3;
const MAX = 18;

/** How long the highlight takes to sweep across the bars while transcribing. */
const SWEEP_MS = 900;

/**
 * White bars that move with the microphone level. `frozen` holds them low and
 * sweeps a highlight across them instead, while the words are transcribed.
 */
export function MicBars({ frozen = false, className }: { frozen?: boolean; className?: string }) {
  const bars = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    let target = 0;
    let smooth = 0;
    let frame = 0;
    const stop = onLevel((level) => {
      target = Math.min(1, Math.max(0, level));
    });
    const tick = (now: number) => {
      // Rise quickly, fall gently.
      const rate = target > smooth ? 0.45 : 0.12;
      smooth += ((frozen ? 0 : target) - smooth) * rate;
      const sweep = ((now % SWEEP_MS) / SWEEP_MS) * (WEIGHTS.length + 6) - 3;
      bars.current.forEach((el, i) => {
        if (!el) return;
        const wobble = 0.78 + 0.22 * Math.sin(now / 115 + i * 1.7);
        const height = MIN + (MAX - MIN) * smooth * WEIGHTS[i] * wobble;
        el.style.height = `${height.toFixed(1)}px`;
        el.style.opacity = frozen ? (0.28 + 0.72 * Math.exp(-((i - sweep) ** 2) / 2.5)).toFixed(2) : "1";
      });
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      stop();
      cancelAnimationFrame(frame);
    };
  }, [frozen]);

  return (
    <div className={cn("flex h-[18px] items-center gap-[2.5px]", className)} aria-hidden>
      {WEIGHTS.map((_, i) => (
        <span
          key={i}
          ref={(el) => {
            bars.current[i] = el;
          }}
          className="w-[3px] rounded-full bg-white"
          style={{ height: MIN }}
        />
      ))}
    </div>
  );
}
