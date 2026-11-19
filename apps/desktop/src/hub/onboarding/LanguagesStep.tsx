import { useState } from "react";
import { SearchInput } from "@/components/ui/input";
import { Group, Row } from "@/components/ui/layout";
import { Select } from "@/components/ui/select";
import { LanguageChip } from "../components/LanguageChip";
import { matchesLanguage, useLanguages } from "../languages";
import { useModels } from "../models";
import { saveSettings } from "../settings";
import { useHub } from "../store";
import { Footer, type StepProps, StepLayout } from "./layout";

/** Radix Select can't hold an empty value. */
const SAME = "same";

export function LanguagesStep({ next, back }: StepProps) {
  const saved = useHub((s) => s.settings?.languages);
  const all = useLanguages();
  const [selected, setSelected] = useState<string[]>(() => saved?.speak.map((l) => l.code) ?? ["en"]);
  const [output, setOutput] = useState<string | null>(saved?.output ?? null);
  const [query, setQuery] = useState("");

  const byCode = new Map((all ?? []).map((l) => [l.code, l]));
  const name = (code: string) => byCode.get(code)?.name ?? code.toUpperCase();
  // Sarvam covers English and the Indian languages, which come first in the list.
  const featured = (all ?? []).filter((l) => l.sarvam);
  const shown = query
    ? (all ?? []).filter((l) => matchesLanguage(l, query)).slice(0, 36)
    : [...featured, ...(all ?? []).filter((l) => !l.sarvam && selected.includes(l.code))];

  const toggle = (code: string) =>
    setSelected((s) => (s.includes(code) ? s.filter((c) => c !== code) : [...s, code]));

  const target = output && !(selected.length === 1 && selected[0] === output) ? output : null;
  const outputs = [
    { value: SAME, label: "The language I speak" },
    { value: "en", label: "English" },
    ...selected.filter((c) => c !== "en").map((c) => ({ value: c, label: name(c) })),
    ...(target && target !== "en" && !selected.includes(target) ? [{ value: target, label: name(target) }] : []),
  ];

  const save = async () => {
    const ok = await saveSettings((s) => {
      const engines = new Map(s.languages.speak.map((l) => [l.code, l.engine]));
      s.languages.speak = selected.map((code) => ({ code, engine: engines.get(code) ?? "local" }));
      s.languages.output = target;
      if (s.languages.current && !selected.includes(s.languages.current)) s.languages.current = null;
    });
    if (!ok) return;
    void useModels.getState().refresh();
    next();
  };

  return (
    <StepLayout
      title="What do you speak?"
      description="Pick every language you might dictate in. Joona works out which one you're using each time."
      footer={
        <Footer
          back={back}
          next={() => void save()}
          disabled={selected.length === 0}
          hint={selected.length === 0 ? "Pick at least one" : summary(selected.map(name), target && name(target))}
        />
      }
    >
      <div className="mb-3 flex items-center justify-between gap-4">
        <p className="text-[12.5px] text-ink-soft">
          {query
            ? `Languages matching “${query.trim()}”`
            : `Indian languages and English. Search for ${(all?.length ?? 0) - featured.length} more.`}
        </p>
        <SearchInput value={query} onChange={setQuery} placeholder="Search languages" />
      </div>
      <div className="flex min-h-[120px] flex-wrap gap-2">
        {all === null && <p className="text-[13px] text-ink-faint">Loading languages…</p>}
        {shown.map((language) => (
          <LanguageChip
            key={language.code}
            language={language}
            on={selected.includes(language.code)}
            onClick={() => toggle(language.code)}
          />
        ))}
        {all && shown.length === 0 && <p className="text-[13px] text-ink-faint">No language matches that.</p>}
      </div>

      <Group className="mt-7">
        <Row
          title="Type in"
          description="Joona can turn your speech into another language as it types. Choose English to speak Hindi and get English."
        >
          <Select
            label="Type in"
            value={target ?? SAME}
            options={outputs}
            onChange={(v) => setOutput(v === SAME ? null : v)}
          />
        </Row>
      </Group>
    </StepLayout>
  );
}

/** `Hindi, Tamil → English`, short enough for the footer. */
function summary(names: string[], output: string | null | undefined): string {
  const from = names.length > 3 ? `${names.slice(0, 2).join(", ")} +${names.length - 2}` : names.join(", ");
  return output ? `${from} → ${output}` : from;
}
