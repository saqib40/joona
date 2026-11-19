import { Check, Cloud, Plus } from "lucide-react";
import { cn } from "@/lib/utils";
import type { Language } from "../languages";

/** A language to pick, in its own script with the English name beside it. */
export function LanguageChip({
  language,
  on,
  onClick,
  adds,
}: {
  language: Language;
  on: boolean;
  onClick: () => void;
  /** Shows a plus, for a chip that adds the language to a list. */
  adds?: boolean;
}) {
  const own = language.native !== language.name;
  return (
    <button
      type="button"
      aria-pressed={adds ? undefined : on}
      onClick={onClick}
      title={language.whisper ? undefined : `${language.name} needs Sarvam, a cloud service.`}
      className={cn(
        "inline-flex h-10 items-center gap-2 rounded-full pl-3.5 pr-4 text-[13.5px] ring-1 transition-[background-color,color,box-shadow]",
        on ? "bg-primary text-primary-ink ring-primary" : "bg-surface-raised text-ink ring-line hover:ring-line-strong",
      )}
    >
      {on && <Check className="size-3.5" strokeWidth={2.75} />}
      {adds && <Plus className="size-3.5 text-ink-faint" strokeWidth={2.5} />}
      <span className="font-medium">{language.native}</span>
      {own && <span className={cn("text-[12px]", on ? "text-primary-ink/70" : "text-ink-faint")}>{language.name}</span>}
      {!language.whisper && <Cloud className={cn("size-3.5", on ? "text-primary-ink/70" : "text-cloud")} />}
    </button>
  );
}
