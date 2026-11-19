import { MotionConfig } from "motion/react";
import { type CSSProperties, useEffect } from "react";
import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Onboarding } from "./onboarding/Onboarding";
import { Dictionary } from "./pages/Dictionary";
import { Home } from "./pages/Home";
import { Insights } from "./pages/Insights";
import { Languages } from "./pages/Languages";
import { Models } from "./pages/Models";
import { Providers } from "./pages/Providers";
import { Snippets } from "./pages/Snippets";
import { Style } from "./pages/Style";
import { Settings } from "./pages/settings/Settings";
import { Sidebar } from "./Sidebar";
import { type Page, useHub } from "./store";
import { applyTheme } from "./theme";
import { TitleBar } from "./TitleBar";

export function App() {
  const theme = useHub((s) => s.settings?.general.theme ?? "system");
  const loaded = useHub((s) => s.settings !== null);
  const onboarded = useHub((s) => s.settings?.onboarded ?? false);
  useEffect(() => applyTheme(theme), [theme]);

  return (
    <MotionConfig reducedMotion="user">
      <TooltipProvider delayDuration={450} skipDelayDuration={150}>
        {!loaded ? (
          // Nothing to show until the settings say which screen it is.
          <TitleBar />
        ) : onboarded ? (
          <Shell />
        ) : (
          <Onboarding />
        )}
        <Toaster
          position="bottom-right"
          offset={18}
          theme={theme}
          style={
            {
              "--normal-bg": "var(--surface-raised)",
              "--normal-text": "var(--ink)",
              "--normal-border": "var(--line)",
              "--border-radius": "12px",
              fontFamily: "var(--font-sans)",
            } as CSSProperties
          }
        />
      </TooltipProvider>
    </MotionConfig>
  );
}

function Shell() {
  const page = useHub((s) => s.page);
  return (
    <div className="flex h-full animate-fade-in">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TitleBar />
        <main className="min-h-0 flex-1 overflow-hidden rounded-tl-[14px] border-l border-t border-line bg-surface">
          <div key={page} className="h-full overflow-y-auto animate-fade-in [scrollbar-gutter:stable]">
            <div className="mx-auto max-w-[800px] px-10 pb-16">
              <PageView page={page} />
            </div>
          </div>
        </main>
      </div>
    </div>
  );
}

function PageView({ page }: { page: Page }) {
  switch (page) {
    case "home":
      return <Home />;
    case "insights":
      return <Insights />;
    case "dictionary":
      return <Dictionary />;
    case "snippets":
      return <Snippets />;
    case "style":
      return <Style />;
    case "languages":
      return <Languages />;
    case "models":
      return <Models />;
    case "providers":
      return <Providers />;
    case "settings":
      return <Settings />;
  }
}
