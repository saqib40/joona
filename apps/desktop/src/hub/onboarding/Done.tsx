import { ArrowRight, Check } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Keys } from "@/components/ui/kbd";
import { Card } from "@/components/ui/layout";
import { languageName } from "../languages";
import { useSarvam } from "../providers";
import { saveSettings } from "../settings";
import { shortcutKeys } from "../shortcut";
import { useHub } from "../store";
import type { StepProps } from "./layout";

export function Done({ back }: StepProps) {
  const settings = useHub((s) => s.settings);
  const sarvam = useSarvam();
  if (!settings) return null;
  const { shortcuts, languages, polish } = settings;
  const trigger = shortcutKeys(shortcuts.trigger);
  const needsProvider = polish.level === "smart" && !polish.provider;
  const needsSarvam = languages.speak.some((l) => l.engine === "sarvam") && !sarvam.hasKey;

  const finish = async () => {
    if (await saveSettings((s) => void (s.onboarded = true))) {
      useHub.getState().navigate(needsProvider || needsSarvam ? "providers" : "home");
    }
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center text-center">
      <span className="grid size-16 place-items-center rounded-full bg-success-soft text-success animate-fade-in">
        <Check className="size-8" strokeWidth={2.5} />
      </span>
      <h1 className="mt-6 font-display text-[52px] leading-none tracking-[-0.02em] text-ink">You're all set</h1>
      <p className="mt-3 max-w-[440px] text-[14px] leading-relaxed text-ink-soft">
        Joona waits in the tray, ready in any app. Here's everything you need to remember.
      </p>
      <Card className="mt-8 w-full max-w-[460px] divide-y divide-line text-left">
        <Tip keys={<Keys keys={trigger} />}>Hold to dictate, let go to type</Tip>
        {shortcuts.doubleTapForHandsFree && <Tip keys={<Keys keys={trigger} />}>Double-tap for hands-free</Tip>}
        <Tip keys={<Keys keys={["Esc"]} />}>Cancel while talking</Tip>
        {shortcuts.pasteLast && <Tip keys={<Keys keys={shortcutKeys(shortcuts.pasteLast)} />}>Paste your last dictation again</Tip>}
        {languages.output && (
          <Tip keys={<span className="text-[12.5px] font-semibold text-ink">→ {languageName(languages.output)}</span>}>
            Whatever you speak comes out in {languageName(languages.output)}
          </Tip>
        )}
      </Card>
      <div className="mt-9 flex items-center gap-3">
        <Button variant="ghost" onClick={back}>
          Back
        </Button>
        <Button variant="primary" size="lg" onClick={() => void finish()}>
          {needsSarvam ? "Add your API keys" : needsProvider ? "Pick an AI model" : "Start using Joona"}
          <ArrowRight />
        </Button>
      </div>
    </div>
  );
}

function Tip({ keys, children }: { keys: ReactNode; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 px-5 py-3">
      <span className="text-[13.5px] text-ink">{children}</span>
      <span className="shrink-0">{keys}</span>
    </div>
  );
}
