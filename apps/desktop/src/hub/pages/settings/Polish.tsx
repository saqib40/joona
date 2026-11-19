import { Sparkles } from "lucide-react";
import { useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Group, Row } from "@/components/ui/layout";
import type { PolishLevel } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { type AiOverview, modelLabel, useAi } from "../../ai";
import { saveSettings } from "../../settings";
import { type Settings, useHub } from "../../store";

const LEVELS: { level: PolishLevel; title: string; description: string }[] = [
  {
    level: "off",
    title: "Off",
    description: "Your words exactly as Joona heard them.",
  },
  {
    level: "basic",
    title: "Basic",
    description: "Takes out ums and repeated words, fixes spacing and capitals, and writes in your style for each app.",
  },
  {
    level: "smart",
    title: "Smart",
    description:
      "An AI model also fixes grammar, follows your corrections like “no, Tuesday”, and turns spoken lists into lists.",
  },
];

/** Where each level does its work. */
function where(level: PolishLevel, overview: AiOverview | null): string {
  if (level !== "smart") return "On this PC";
  const ready = overview?.ready;
  if (!ready) return "On this PC, or with your provider";
  return ready.cloud ? `Your text goes to ${ready.name}` : "On this PC";
}

export function Polish({ settings }: { settings: Settings }) {
  const { level, provider } = settings.polish;
  const overview = useAi((s) => s.overview);
  useEffect(() => void useAi.getState().refresh(), [provider]);
  const ready = overview?.ready ?? null;
  const picked = overview?.providers.find((p) => p.id === provider);
  const providers = () => useHub.getState().navigate("providers");

  return (
    <>
      <div role="radiogroup" aria-label="AI polish" className="grid grid-cols-3 gap-3">
        {LEVELS.map((option) => {
          const checked = option.level === level;
          return (
            <button
              key={option.level}
              type="button"
              role="radio"
              aria-checked={checked}
              onClick={() => void saveSettings((s) => void (s.polish.level = option.level))}
              className={cn(
                "flex flex-col rounded-xl bg-surface-raised p-4 text-left shadow-soft ring-1 transition-[box-shadow,background-color]",
                checked ? "ring-2 ring-ink" : "ring-line hover:bg-fill/60",
              )}
            >
              <span className="flex items-center justify-between">
                <span className="text-[14px] font-semibold text-ink">{option.title}</span>
                <span
                  className={cn(
                    "grid size-4 place-items-center rounded-full ring-1 ring-line-strong",
                    checked && "bg-ink ring-ink",
                  )}
                >
                  {checked && <span className="size-1.5 rounded-full bg-surface-raised" />}
                </span>
              </span>
              <span className="mt-1.5 flex-1 text-[12.5px] leading-snug text-ink-soft">{option.description}</span>
              <span className="mt-3 text-[11.5px] font-medium text-ink-faint">{where(option.level, overview)}</span>
            </button>
          );
        })}
      </div>
      {level === "smart" && overview && (
        <Group>
          {ready ? (
            <Row
              title={`Smart polish uses ${ready.name}`}
              description={
                ready.cloud
                  ? `${modelLabel(ready, overview)}. The bar shows a cloud badge when your text goes to ${ready.name}.`
                  : `${modelLabel(ready, overview)}, on this PC.`
              }
            >
              <Button variant="ghost" onClick={providers}>
                Change
              </Button>
            </Row>
          ) : picked && overview.problem ? (
            <Row
              title={`Smart polish can't use ${picked.name} yet`}
              description={`${overview.problem} Until it can, Joona uses Basic.`}
            >
              <Button variant="primary" onClick={providers}>
                Fix on Providers
              </Button>
            </Row>
          ) : (
            <Row
              title="Smart polish needs an AI model"
              description="Use Joona's free model on this PC, or your own key for OpenAI, Anthropic, Groq and others. Until then Joona uses Basic."
            >
              <Button variant="primary" onClick={providers}>
                <Sparkles />
                Pick a model
              </Button>
            </Row>
          )}
        </Group>
      )}
      <p className="px-1 text-[12.5px] leading-relaxed text-ink-soft">
        Polish never changes what you meant. Your dictionary and snippets apply at every level, and History keeps
        what Joona heard so you can compare.
      </p>
    </>
  );
}
