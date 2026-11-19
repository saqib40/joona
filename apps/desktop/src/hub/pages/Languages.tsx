import { Cloud, Plus, X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/components/ui/input";
import { Keys } from "@/components/ui/kbd";
import { Group, PageHeader, Row } from "@/components/ui/layout";
import { Segmented } from "@/components/ui/segmented";
import { Select } from "@/components/ui/select";
import { commands, type LanguageEngine, type RoutePreview_Serialize as RoutePreview, type Script } from "@/lib/bindings";
import { useAi } from "../ai";
import { LanguageChip } from "../components/LanguageChip";
import { useSarvamSheet } from "../components/SarvamSheet";
import { listNames } from "../format";
import { useLoad } from "../hooks";
import { type Language, matchesLanguage, useLanguages } from "../languages";
import { useModels } from "../models";
import { useSarvam } from "../providers";
import { saveSettings } from "../settings";
import { shortcutKeys } from "../shortcut";
import { type Settings, useHub } from "../store";
import { type SttChoice, useStt } from "../stt";

/** Radix Select can't hold an empty value. */
const SAME = "same";
const AUTO = "auto";
const OFF = "off";

const TITLE = "Languages";
const DESCRIPTION = "The languages you speak, and the one Joona types in.";

export function Languages() {
  const settings = useHub((s) => s.settings);
  const all = useLanguages();
  if (!settings || !all) return <PageHeader title={TITLE} description={DESCRIPTION} />;
  return <LanguagesPage settings={settings} all={all} />;
}

function LanguagesPage({ settings, all }: { settings: Settings; all: Language[] }) {
  const { languages, shortcuts } = settings;
  const [adding, setAdding] = useState(false);
  const overview = useModels((s) => s.overview);
  const sarvam = useSarvam();
  const ai = useAi((s) => s.overview?.ready?.provider ?? null);
  const service = useStt((s) => s.overview?.ready ?? null);
  const { request, sheet } = useSarvamSheet();
  useEffect(() => void useModels.getState().refresh(), []);
  const installed = overview?.models
    .filter((m) => m.installed)
    .map((m) => m.id)
    .join(" ");
  const routes = useLoad(
    () => commands.languageRoutes(settings),
    [
      languages,
      settings.models,
      settings.polish,
      settings.transcription,
      settings.privacy.consented,
      installed,
      sarvam.hasKey,
      ai,
    ],
  ).data;
  const aiTranslates = ai !== null && settings.polish.translate;

  const byCode = new Map(all.map((l) => [l.code, l]));
  const name = (code: string) => byCode.get(code)?.name ?? code.toUpperCase();
  const modelName = (id: string | null) => overview?.models.find((m) => m.id === id)?.name ?? id ?? "Your model";
  const spoken = languages.speak.map((l) => l.code);
  const nonLatin = spoken.map((c) => byCode.get(c)).filter((l): l is Language => !!l && !l.latin);

  // English, the other languages you speak, then the Indian languages Sarvam can translate into.
  const targets = [
    "en",
    ...spoken.filter((c) => c !== "en"),
    ...all.filter((l) => l.sarvam && l.code !== "en" && !spoken.includes(l.code)).map((l) => l.code),
  ];
  const options = (saved: string | null) =>
    (saved && !targets.includes(saved) ? [...targets, saved] : targets).map((c) => ({ value: c, label: name(c) }));

  const add = (code: string) => void saveSettings((s) => void s.languages.speak.push({ code, engine: "local" }));
  const remove = (code: string) =>
    void saveSettings((s) => {
      s.languages.speak = s.languages.speak.filter((l) => l.code !== code);
      if (s.languages.current === code) s.languages.current = null;
    });
  const setEngine = (code: string, engine: LanguageEngine) => {
    const change = (s: Settings) => {
      const choice = s.languages.speak.find((l) => l.code === code);
      if (choice) choice.engine = engine;
    };
    if (engine === "sarvam") request(change);
    else void saveSettings(change);
  };
  const onSarvam = languages.speak.some((l) => l.engine === "sarvam");

  return (
    <>
      <PageHeader title={TITLE} description={DESCRIPTION} />

      <Group
        title="I speak"
        description="Joona listens for these and works out which one you're using."
        action={
          !adding && (
            <Button size="sm" onClick={() => setAdding(true)}>
              <Plus />
              Add a language
            </Button>
          )
        }
      >
        {languages.speak.map((choice) => (
          <SpokenLanguage
            key={choice.code}
            code={choice.code}
            language={byCode.get(choice.code)}
            engine={choice.engine}
            sarvamKey={sarvam.hasKey}
            service={service}
            only={languages.speak.length === 1}
            onEngine={(engine) => setEngine(choice.code, engine)}
            onRemove={() => remove(choice.code)}
          />
        ))}
        {adding && <AddLanguages all={all} spoken={spoken} onAdd={add} onDone={() => setAdding(false)} />}
      </Group>

      <Group title="When you dictate" className="mt-8">
        <Row
          title="Type in"
          description={
            <RouteNote route={routes?.dictate} target={languages.output} name={name} modelName={modelName}>
              Joona types in the language you speak.
            </RouteNote>
          }
        >
          <Select
            label="Type in"
            value={languages.output ?? SAME}
            options={[{ value: SAME, label: "The language I speak" }, ...options(languages.output)]}
            onChange={(v) => void saveSettings((s) => void (s.languages.output = v === SAME ? null : v))}
          />
        </Row>
        {spoken.length > 1 && (
          <Row
            title="Listen for"
            description="Joona works out which of your languages you're speaking. Pick one if it guesses wrong."
          >
            <Select
              label="Listen for"
              value={languages.current ?? AUTO}
              options={[{ value: AUTO, label: "Any of my languages" }, ...spoken.map((c) => ({ value: c, label: name(c) }))]}
              onChange={(v) => void saveSettings((s) => void (s.languages.current = v === AUTO ? null : v))}
            />
          </Row>
        )}
        {nonLatin.length > 0 && (
          <Row title="Script" description={scriptNote(languages.script, nonLatin[0], sarvam.ready || aiTranslates)}>
            <Segmented
              label="Script"
              value={languages.script}
              onChange={(script) => void saveSettings((s) => void (s.languages.script = script))}
              options={[
                { value: "native", label: "Native" },
                ...(onSarvam || languages.script === "mixed" ? [{ value: "mixed" as const, label: "Mixed" }] : []),
                { value: "roman", label: "Roman" },
              ]}
            />
          </Row>
        )}
      </Group>
      {sheet}

      <Group title="Alternate shortcut" className="mt-8">
        <Row
          title={
            <span className="inline-flex flex-wrap items-center gap-1.5">
              Hold <Keys keys={[...shortcutKeys(shortcuts.trigger), "Shift"]} /> to type in
            </span>
          }
          description={
            shortcuts.trigger.shift ? (
              "Not available while your shortcut uses Shift."
            ) : languages.alternate ? (
              <RouteNote route={routes?.alternate ?? undefined} target={languages.alternate} name={name} modelName={modelName}>
                Joona types in the language you speak.
              </RouteNote>
            ) : (
              "A second language for when you need it. For example, keep Hindi as Hindi, and add Shift to get English."
            )
          }
        >
          <Select
            label="Alternate shortcut language"
            disabled={shortcuts.trigger.shift}
            value={languages.alternate ?? OFF}
            options={[{ value: OFF, label: "Off" }, ...options(languages.alternate)]}
            onChange={(v) => void saveSettings((s) => void (s.languages.alternate = v === OFF ? null : v))}
          />
        </Row>
      </Group>
    </>
  );
}

function SpokenLanguage({
  code,
  language,
  engine,
  sarvamKey,
  service,
  only,
  onEngine,
  onRemove,
}: {
  code: string;
  language: Language | undefined;
  engine: LanguageEngine;
  sarvamKey: boolean;
  /** The speech-to-text service that hears what doesn't go to Sarvam. */
  service: SttChoice | null;
  only: boolean;
  onEngine: (engine: LanguageEngine) => void;
  onRemove: () => void;
}) {
  const label = language?.name ?? code.toUpperCase();
  const away = service?.cloud ? service.name : null;
  return (
    <Row
      title={
        <span className="flex items-baseline gap-2">
          {language && language.native !== language.name && <span>{language.native}</span>}
          <span className={language && language.native !== language.name ? "text-[12.5px] font-normal text-ink-faint" : ""}>
            {label}
          </span>
        </span>
      }
      description={quality(language, engine, sarvamKey, service)}
    >
      {language?.sarvam && (
        <Segmented
          label={`Where ${label} is turned into text`}
          value={engine}
          onChange={onEngine}
          options={[
            away ? { value: "local", label: away, icon: <Cloud /> } : { value: "local", label: "This PC" },
            { value: "sarvam", label: "Sarvam", icon: <Cloud /> },
          ]}
        />
      )}
      <Button variant="ghost" size="icon-sm" aria-label={`Remove ${label}`} disabled={only} onClick={onRemove}>
        <X />
      </Button>
    </Row>
  );
}

function quality(
  language: Language | undefined,
  engine: LanguageEngine,
  sarvamKey: boolean,
  service: SttChoice | null,
): string {
  if (engine === "sarvam" && language) {
    return sarvamKey
      ? "Sent to Sarvam AI to be turned into text."
      : "Sent to Sarvam AI once you add your key on the Providers page.";
  }
  if (service) {
    return service.cloud ? `Sent to ${service.name} to be turned into text.` : "Turned into text by your own server.";
  }
  if (!language) return "Joona doesn't know this language, so it's left to your model.";
  switch (language.local) {
    case "good":
      return "Works well on this PC.";
    case "fair":
      return "Works on this PC, with some mistakes. Bigger models make fewer.";
    case "weak":
      return `Models on this PC often get ${language.name} wrong.${language.sarvam ? " Sarvam is much more accurate." : ""}`;
    case "none":
      return `Models on this PC don't understand ${language.name}. Choose Sarvam to use it.`;
  }
}

/** How speech becomes text in the chosen language. */
function RouteNote({
  route,
  target,
  name,
  modelName,
  children,
}: {
  route: RoutePreview | undefined;
  target: string | null;
  name: (code: string) => string;
  modelName: (id: string | null) => string;
  /** What to say when the text stays in the spoken language. */
  children: ReactNode;
}) {
  if (!route) return null;
  if (route.problem) return <span className="text-danger">{route.problem}</span>;
  const into = target ? name(target) : "your language";
  switch (route.translation) {
    case "none":
      return <>{children}</>;
    case "speech":
      if (route.cloud) return <>{route.cloud} turns your speech into English.</>;
      return (
        <>
          {route.service
            ? `${route.service} turns your speech into English.`
            : `${modelName(route.model)} turns your speech into English, right on this PC.`}
          {route.sarvam.length > 0 && ` Sarvam does it for ${listNames(route.sarvam.map(name))}.`}
        </>
      );
    case "text":
      if (route.translator) return <>Joona writes down what you say, then {route.translator} translates it into {into}.</>;
      return (
        <span className="text-accent-ink">
          Joona writes down what you say, then translates it into {into}. This needs Sarvam or an AI model, which
          you can set up on the Providers page.
        </span>
      );
  }
}

/**
 * What the script choice means for the first spoken language that isn't
 * written in English letters. `writer` says whether Sarvam or an AI model can
 * write it in English letters.
 */
function scriptNote(script: Script, language: Language, writer: boolean): string {
  const example = language.code === "hi";
  switch (script) {
    case "native":
      return `${language.name} comes out as ${language.native}. Choose Roman to get English letters instead.`;
    case "mixed":
      return `English words stay in English letters and the rest is in ${language.native}${example ? ", like “मेरा phone number है”" : ""}. Only Sarvam writes this way.`;
    case "roman":
      return `${language.name} comes out in English letters${example ? ", like “kal milte hain”" : ""}.${
        writer ? "" : ` This needs Sarvam or an AI model; until you add one, Joona uses ${language.native}.`
      }`;
  }
}

function AddLanguages({
  all,
  spoken,
  onAdd,
  onDone,
}: {
  all: Language[];
  spoken: string[];
  onAdd: (code: string) => void;
  onDone: () => void;
}) {
  const [query, setQuery] = useState("");
  const shown = all.filter((l) => !spoken.includes(l.code) && matchesLanguage(l, query)).slice(0, query ? 40 : 22);
  return (
    <div className="space-y-3 bg-fill/50 px-4 py-4">
      <div className="flex items-center justify-between gap-3">
        <SearchInput autoFocus value={query} onChange={setQuery} placeholder="Search languages" className="w-64" />
        <Button size="sm" variant="ghost" onClick={onDone}>
          Done
        </Button>
      </div>
      <div className="flex flex-wrap gap-2">
        {shown.map((language) => (
          <LanguageChip key={language.code} language={language} on={false} adds onClick={() => onAdd(language.code)} />
        ))}
        {shown.length === 0 && <p className="py-2 text-[13px] text-ink-faint">No language matches that.</p>}
      </div>
    </div>
  );
}
