import {
  BookA,
  ChartNoAxesColumn,
  Cloud,
  Cpu,
  Download,
  House,
  KeyRound,
  Languages,
  type LucideIcon,
  Palette,
  Settings,
  ShieldCheck,
  TextQuote,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { type AiChoice, useAi } from "./ai";
import { languageName } from "./languages";
import { type Page, type Settings as SettingsType, useHub } from "./store";
import { type SttChoice, useStt } from "./stt";
import { useUpdates } from "./updates";

const NAV: { page: Page; label: string; icon: LucideIcon }[] = [
  { page: "home", label: "Home", icon: House },
  { page: "insights", label: "Insights", icon: ChartNoAxesColumn },
  { page: "dictionary", label: "Dictionary", icon: BookA },
  { page: "snippets", label: "Snippets", icon: TextQuote },
  { page: "style", label: "Style", icon: Palette },
  { page: "languages", label: "Languages", icon: Languages },
  { page: "models", label: "Models", icon: Cpu },
  { page: "providers", label: "Providers", icon: KeyRound },
];

export function Sidebar() {
  return (
    <nav aria-label="Joona" className="flex w-[212px] shrink-0 flex-col px-3 pb-3">
      <div data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-2.5 px-2">
        <Logo />
      </div>
      <ul className="mt-1 space-y-0.5">
        {NAV.map((item) => (
          <NavItem key={item.page} {...item} />
        ))}
      </ul>
      <div data-tauri-drag-region className="flex-1" />
      <UpdateChip />
      <ul className="space-y-0.5">
        <NavItem page="settings" label="Settings" icon={Settings} />
      </ul>
      <PrivacyChip />
    </nav>
  );
}

function UpdateChip() {
  const version = useUpdates((s) => (s.status?.state === "available" ? s.status.version : null));
  if (!version) return null;
  return (
    <button
      type="button"
      onClick={() => useHub.getState().navigate("settings/about")}
      className="mb-2 flex w-full items-center gap-2 rounded-lg bg-accent-soft px-2.5 py-2 text-left text-[12px] font-medium leading-tight text-ink transition-colors hover:brightness-[0.98]"
    >
      <Download className="size-3.5 shrink-0" strokeWidth={2} />
      <span className="min-w-0 flex-1">Joona {version} is available</span>
    </button>
  );
}

export function Logo() {
  return (
    <span className="pointer-events-none flex items-center gap-2.5">
      <span className="relative grid size-[26px] place-items-center rounded-[8px] bg-primary shadow-soft">
        <span className="flex items-end gap-[2px]">
          {[5, 9, 6].map((h, i) => (
            <span key={i} className="w-[2.5px] rounded-full bg-accent" style={{ height: h }} />
          ))}
        </span>
      </span>
      <span className="font-display text-[24px] leading-none tracking-[-0.01em] text-ink">Joona</span>
    </span>
  );
}

function NavItem({ page, label, icon: Icon }: { page: Page; label: string; icon: LucideIcon }) {
  const active = useHub((s) => s.page === page);
  return (
    <li>
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        onClick={() => useHub.getState().navigate(page)}
        className={cn(
          "flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13.5px] transition-colors",
          active
            ? "bg-surface-raised font-medium text-ink shadow-soft ring-1 ring-line"
            : "text-ink-soft hover:bg-ink/[0.05] hover:text-ink",
        )}
      >
        <Icon className="size-4 shrink-0" strokeWidth={active ? 2.1 : 1.8} />
        {label}
      </button>
    </li>
  );
}

/**
 * Where audio and text go, in a few words. `ai` is the model Smart polish
 * uses, and `stt` the service that hears dictations.
 */
export function privacySummary(
  settings: SettingsType | null,
  ai: AiChoice | null = null,
  stt: SttChoice | null = null,
): { cloud: boolean; text: string } {
  if (!settings) return { cloud: false, text: "Everything stays on this PC" };
  if (stt?.cloud) return { cloud: true, text: `Cloud: ${stt.name} hears your dictations` };
  const sarvam = settings.languages.speak.filter((l) => l.engine === "sarvam").map((l) => languageName(l.code));
  if (sarvam.length > 0) {
    const list = sarvam.length > 2 ? `${sarvam.slice(0, 2).join(", ")} +${sarvam.length - 2}` : sarvam.join(" and ");
    return { cloud: true, text: `Cloud: Sarvam for ${list}` };
  }
  if (settings.polish.level === "smart" && ai?.cloud) return { cloud: true, text: `Cloud: Smart polish with ${ai.name}` };
  return { cloud: false, text: "Everything stays on this PC" };
}

function PrivacyChip() {
  const settings = useHub((s) => s.settings);
  const ai = useAi((s) => s.overview?.ready ?? null);
  const stt = useStt((s) => s.overview?.ready ?? null);
  const { cloud, text } = privacySummary(settings, ai, stt);
  const Icon = cloud ? Cloud : ShieldCheck;
  return (
    <button
      type="button"
      onClick={() => useHub.getState().navigate("settings/privacy")}
      title="Data & Privacy settings"
      className={cn(
        "mt-3 flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-[12px] font-medium leading-tight transition-colors",
        cloud ? "bg-cloud-soft text-cloud hover:brightness-[0.98]" : "bg-success-soft text-success hover:brightness-[0.98]",
      )}
    >
      <Icon className="size-3.5 shrink-0" strokeWidth={2} />
      <span className="min-w-0 flex-1">{text}</span>
    </button>
  );
}
