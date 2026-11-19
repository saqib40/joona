import { commands, type Language } from "@/lib/bindings";
import { useLoad } from "./hooks";

export type { Language };

const names = (() => {
  try {
    return new Intl.DisplayNames(["en"], { type: "language" });
  } catch {
    return null;
  }
})();

/** `ta` becomes Tamil. Unknown codes are shown in capitals. */
export function languageName(code: string): string {
  try {
    const name = names?.of(code);
    if (name && name.toLowerCase() !== code.toLowerCase()) return name;
  } catch {
    // Not a valid language tag.
  }
  return code.toUpperCase();
}

/** `HI → EN`, or `HI` when the text stays in the spoken language. */
export function routeLabel(language: string | null, output: string | null): string | null {
  if (!language && !output) return null;
  const from = (language ?? "auto").toUpperCase();
  return output && output !== language ? `${from} → ${output.toUpperCase()}` : from;
}

let list: Promise<Language[]> | null = null;

/** Every language Joona knows, from Rust. Loaded once. */
export function loadLanguages(): Promise<Language[]> {
  list ??= commands.languages().catch((e) => {
    list = null;
    throw e;
  });
  return list;
}

export function useLanguages(): Language[] | null {
  return useLoad(loadLanguages, []).data;
}

/** Matches a language by its English name, its own name or its code. */
export function matchesLanguage(language: Language, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    language.name.toLowerCase().includes(q) ||
    language.native.toLowerCase().includes(q) ||
    language.code.toLowerCase() === q
  );
}
