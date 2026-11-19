import { create } from "zustand";
import type { AppInfo_Serialize as AppInfo, Settings_Serialize as Settings } from "@/lib/bindings";

export type { AppInfo, Settings };

export const PAGES = [
  "home",
  "insights",
  "dictionary",
  "snippets",
  "style",
  "languages",
  "models",
  "providers",
  "settings",
] as const;
export type Page = (typeof PAGES)[number];

export const SECTIONS = ["general", "polish", "insertion", "privacy", "experimental", "about"] as const;
export type Section = (typeof SECTIONS)[number];

interface HubStore {
  page: Page;
  section: Section;
  settings: Settings | null;
  info: AppInfo | null;
  /** Saves still on their way to Rust. Settings events are ignored meanwhile. */
  saving: number;
  /** `models`, `settings`, or `settings/privacy`. Unknown pages are ignored. */
  navigate: (to: string) => void;
}

export const useHub = create<HubStore>((set) => ({
  page: "home",
  section: "general",
  settings: null,
  info: null,
  saving: 0,
  navigate: (to) => {
    const [page, section] = to.split("/");
    if (!isPage(page)) return;
    set(isSection(section) ? { page, section } : { page });
  },
}));

const isPage = (value: string | undefined): value is Page => PAGES.includes(value as Page);
const isSection = (value: string | undefined): value is Section => SECTIONS.includes(value as Section);
