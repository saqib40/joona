import type { Theme } from "@/lib/bindings";

const THEME_KEY = "joona.theme";
const MICA_KEY = "joona.mica";

/**
 * Applies the theme now and, for `system`, whenever Windows switches. Stored
 * so the next start paints in the right theme before React runs.
 */
export function applyTheme(theme: Theme): () => void {
  remember(THEME_KEY, theme);
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    const dark = theme === "dark" || (theme === "system" && media.matches);
    document.documentElement.classList.toggle("dark", dark);
  };
  apply();
  if (theme !== "system") return () => {};
  media.addEventListener("change", apply);
  return () => media.removeEventListener("change", apply);
}

/** With Mica, the window background shows through the sidebar. */
export function applyMica(on: boolean) {
  document.documentElement.dataset.mica = String(on);
  remember(MICA_KEY, String(on));
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage can be unavailable; the theme still applies for this session.
  }
}
