import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { cn, isMac } from "@/lib/utils";
import { saveSettings } from "../settings";
import { Logo } from "../Sidebar";
import { MacTrafficLights } from "../components/MacTrafficLights";
import { TitleBar } from "../TitleBar";
import { Boosts } from "./Boosts";
import { Done } from "./Done";
import { LanguagesStep } from "./LanguagesStep";
import type { StepProps } from "./layout";
import { MicrophoneStep } from "./MicrophoneStep";
import { ModelStep } from "./ModelStep";
import { Practice } from "./Practice";
import { Welcome } from "./Welcome";

const STEPS: { id: string; view: (props: StepProps) => ReactNode }[] = [
  { id: "welcome", view: (p) => <Welcome {...p} /> },
  { id: "languages", view: (p) => <LanguagesStep {...p} /> },
  { id: "microphone", view: (p) => <MicrophoneStep {...p} /> },
  { id: "model", view: (p) => <ModelStep {...p} /> },
  { id: "practice", view: (p) => <Practice {...p} /> },
  { id: "boosts", view: (p) => <Boosts {...p} /> },
  { id: "done", view: (p) => <Done {...p} /> },
];

const slide = {
  enter: (direction: number) => ({ opacity: 0, x: direction * 28 }),
  center: { opacity: 1, x: 0 },
  exit: (direction: number) => ({ opacity: 0, x: direction * -28 }),
};

/** First-run setup. It fills the Hub until it's finished or skipped. */
export function Onboarding() {
  const [index, setIndex] = useState(0);
  const [direction, setDirection] = useState(1);
  const go = (to: number) => {
    setDirection(to > index ? 1 : -1);
    setIndex(Math.max(0, Math.min(STEPS.length - 1, to)));
  };
  const step = STEPS[index];
  const middle = index > 0 && index < STEPS.length - 1;

  return (
    <div className="flex h-full flex-col bg-surface">
      <header
        data-tauri-drag-region
        onDoubleClick={() => void getCurrentWindow().toggleMaximize()}
        className="flex h-12 shrink-0 items-center"
      >
        {isMac && <MacTrafficLights className="pl-4 pr-1" />}
        <div data-tauri-drag-region className="flex flex-1 items-center gap-6 pl-5">
          <Logo />
          {middle && <Progress index={index - 1} total={STEPS.length - 2} />}
        </div>
        {middle && (
          <button
            type="button"
            onClick={() => void saveSettings((s) => void (s.onboarded = true))}
            className="mr-3 rounded-md px-2.5 py-1.5 text-[12.5px] font-medium text-ink-faint transition-colors hover:bg-fill hover:text-ink"
          >
            Skip setup
          </button>
        )}
        <TitleBar />
      </header>
      <div className="relative min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        <AnimatePresence mode="wait" custom={direction} initial={false}>
          <motion.div
            key={step.id}
            custom={direction}
            variants={slide}
            initial="enter"
            animate="center"
            exit="exit"
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
            className="mx-auto flex min-h-full w-full max-w-[680px] flex-col px-10 pb-7"
          >
            {step.view({ next: () => go(index + 1), back: () => go(index - 1) })}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

function Progress({ index, total }: { index: number; total: number }) {
  return (
    <div
      role="progressbar"
      aria-label="Setup"
      aria-valuemin={1}
      aria-valuemax={total}
      aria-valuenow={index + 1}
      className="flex items-center gap-1.5"
    >
      {Array.from({ length: total }, (_, i) => (
        <span
          key={i}
          className={cn(
            "h-1 rounded-full transition-all duration-300",
            i === index ? "w-6 bg-ink" : i < index ? "w-3 bg-ink/45" : "w-3 bg-fill-strong",
          )}
        />
      ))}
    </div>
  );
}
