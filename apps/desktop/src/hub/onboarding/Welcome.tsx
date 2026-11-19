import { AppWindow, ArrowRight, Languages, type LucideIcon, ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Keys } from "@/components/ui/kbd";
import { shortcutKeys } from "../shortcut";
import { useHub } from "../store";
import type { StepProps } from "./layout";

const FEATURES: { icon: LucideIcon; title: string; text: string }[] = [
  { icon: AppWindow, title: "Every app", text: "Email, chat, documents, code. Anywhere you can type, you can talk." },
  { icon: Languages, title: "Your languages", text: "Speak Hindi or Tamil and get English, or keep your own language." },
  { icon: ShieldCheck, title: "Private", text: "Speech turns into text on this PC. Nothing leaves it unless you say so." },
];

export function Welcome({ next }: StepProps) {
  const name = useHub((s) => s.info?.firstName);
  const trigger = useHub((s) => s.settings?.shortcuts.trigger);
  return (
    <div className="flex flex-1 flex-col items-center justify-center pt-2 text-center">
      <VoicePill />
      <h1 className="mt-9 font-display text-[60px] leading-[0.98] tracking-[-0.02em] text-ink">
        Speak naturally.
        <br />
        Joona types.
      </h1>
      <p className="mt-5 max-w-[470px] text-[14.5px] leading-relaxed text-ink-soft">
        {name && `Welcome, ${name}. `}Hold {trigger ? <Keys keys={shortcutKeys(trigger)} /> : "a shortcut"} in any app
        and talk. Your words appear as text, in the language you speak or in English.
      </p>
      <ul className="mt-9 grid w-full grid-cols-3 gap-3 text-left">
        {FEATURES.map(({ icon: Icon, title, text }) => (
          <li key={title} className="rounded-xl bg-surface-raised p-4 shadow-soft ring-1 ring-line">
            <Icon className="size-[18px] text-accent-ink" strokeWidth={1.9} />
            <p className="mt-2.5 text-[13.5px] font-semibold text-ink">{title}</p>
            <p className="mt-1 text-[12.5px] leading-snug text-ink-soft">{text}</p>
          </li>
        ))}
      </ul>
      <Button variant="primary" size="lg" className="mt-10" onClick={next}>
        Get started
        <ArrowRight />
      </Button>
      <p className="mt-3 text-[12px] text-ink-faint">Free and open source. Setup takes about two minutes.</p>
    </div>
  );
}

const BARS = [0.45, 0.8, 0.55, 1, 0.7, 0.9, 0.5, 0.75, 0.4];

/** The Joona Bar while listening, as a picture. */
function VoicePill() {
  return (
    <div
      aria-hidden
      className="flex h-12 items-center gap-[5px] rounded-full bg-[oklch(0.18_0.01_60)] px-6 shadow-lifted ring-1 ring-white/10"
    >
      {BARS.map((height, i) => (
        <span
          key={i}
          className="w-[3.5px] origin-center animate-voice rounded-full bg-accent"
          style={{ height: `${height * 26}px`, animationDelay: `${(i % 5) * -0.21}s`, animationDuration: `${0.9 + (i % 3) * 0.17}s` }}
        />
      ))}
    </div>
  );
}
