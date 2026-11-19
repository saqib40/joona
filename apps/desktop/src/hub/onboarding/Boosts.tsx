import { Check, Cloud, type LucideIcon, Sparkles } from "lucide-react";
import type { ReactNode } from "react";
import { Card } from "@/components/ui/layout";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { useSarvamSheet } from "../components/SarvamSheet";
import { listNames as list } from "../format";
import { type Language, useLanguages } from "../languages";
import { saveSettings } from "../settings";
import { type Settings, useHub } from "../store";
import { Footer, type StepProps, StepLayout } from "./layout";

export function Boosts({ next, back }: StepProps) {
  const settings = useHub((s) => s.settings);
  const all = useLanguages();
  const { request, sheet } = useSarvamSheet();
  if (!settings) return null;

  const smart = settings.polish.level === "smart";
  const known = new Map((all ?? []).map((l) => [l.code, l]));
  // Sarvam is for the Indian languages; English already works well on this PC.
  const indian = settings.languages.speak
    .map((choice) => ({ choice, language: known.get(choice.code) }))
    .filter((x): x is { choice: typeof x.choice; language: Language } => !!x.language?.sarvam && x.choice.code !== "en");
  const sent = indian.filter((x) => x.choice.engine === "sarvam").map((x) => x.language.name);
  const onlySarvam = indian.filter((x) => !x.language.whisper).map((x) => x.language.name);

  const setEngine = (code: string, sarvam: boolean) => {
    const change = (s: Settings) => {
      const choice = s.languages.speak.find((l) => l.code === code);
      if (choice) choice.engine = sarvam ? "sarvam" : "local";
    };
    if (sarvam) request(change);
    else void saveSettings(change);
  };

  return (
    <StepLayout
      title="Optional boosts"
      description="Joona works fully on this PC. These extras use services you pick, and you can change them at any time."
      footer={<Footer back={back} next={next} />}
    >
      <div className="space-y-4">
        <Boost
          icon={Sparkles}
          title="Smart polish"
          on={smart}
          onChange={(on) => void saveSettings((s) => void (s.polish.level = on ? "smart" : "basic"))}
          text="An AI model fixes grammar, follows corrections like “no, Tuesday”, and turns spoken lists into lists. Use a free model on this PC, or your own key for OpenAI, Anthropic, Groq and others."
        >
          {smart && <Note>You'll pick the AI model on the Providers page when setup is done.</Note>}
        </Boost>

        {indian.length > 0 && (
          <Boost
            icon={Cloud}
            title="Sarvam for Indian languages"
            text="Sarvam AI's speech models understand Indian languages, accents and Hinglish better than the models that run on this PC."
          >
            <div className="mt-4 flex flex-wrap gap-2">
              {indian.map(({ choice, language }) => {
                const on = choice.engine === "sarvam";
                return (
                  <button
                    key={language.code}
                    type="button"
                    aria-pressed={on}
                    onClick={() => setEngine(language.code, !on)}
                    className={cn(
                      "inline-flex h-9 items-center gap-2 rounded-full px-3.5 text-[13px] ring-1 transition-colors",
                      on ? "bg-cloud-soft text-cloud ring-cloud/40" : "bg-surface text-ink ring-line hover:ring-line-strong",
                    )}
                  >
                    {on && <Check className="size-3.5" strokeWidth={2.75} />}
                    {language.native}
                    {language.native !== language.name && (
                      <span className={on ? "text-cloud/70" : "text-ink-faint"}>{language.name}</span>
                    )}
                  </button>
                );
              })}
            </div>
            {onlySarvam.length > 0 && (
              <Note>
                {list(onlySarvam)} {onlySarvam.length === 1 ? "needs" : "need"} Sarvam: the models on this PC don't
                understand {onlySarvam.length === 1 ? "it" : "them"} yet.
              </Note>
            )}
            <div className="mt-4 rounded-lg bg-fill px-4 py-3 text-[12.5px] leading-relaxed text-ink-soft">
              <p className="font-medium text-ink">What this means for your privacy</p>
              <p className="mt-1">
                {sent.length > 0
                  ? `Recordings in ${list(sent)} are sent to Sarvam AI, an Indian company, to be turned into text.`
                  : "Only recordings in the languages you pick are sent to Sarvam AI, an Indian company, to be turned into text."}{" "}
                Everything else stays on this PC. Joona shows a cloud badge whenever audio leaves it. Picking a
                language asks for your Sarvam API key.
              </p>
            </div>
          </Boost>
        )}
      </div>
      {sheet}
    </StepLayout>
  );
}

function Boost({
  icon: Icon,
  title,
  text,
  on,
  onChange,
  children,
}: {
  icon: LucideIcon;
  title: string;
  text: string;
  on?: boolean;
  onChange?: (on: boolean) => void;
  children?: ReactNode;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-start gap-4">
        <span className="grid size-10 shrink-0 place-items-center rounded-full bg-accent-soft text-accent-ink">
          <Icon className="size-[18px]" strokeWidth={2} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-4">
            <p className="text-[14.5px] font-semibold text-ink">{title}</p>
            {onChange && <Switch aria-label={title} checked={!!on} onCheckedChange={onChange} />}
          </div>
          <p className="mt-1 text-[13px] leading-relaxed text-ink-soft">{text}</p>
          {children}
        </div>
      </div>
    </Card>
  );
}

function Note({ children }: { children: ReactNode }) {
  return <p className="mt-3 text-[12.5px] leading-relaxed text-accent-ink">{children}</p>;
}
