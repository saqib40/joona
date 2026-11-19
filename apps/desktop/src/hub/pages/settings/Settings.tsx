import { motion } from "motion/react";
import { PageHeader } from "@/components/ui/layout";
import { cn } from "@/lib/utils";
import { type Section, useHub } from "../../store";
import { About } from "./About";
import { Experimental } from "./Experimental";
import { General } from "./General";
import { Insertion } from "./Insertion";
import { Polish } from "./Polish";
import { Privacy } from "./Privacy";

const TABS: { section: Section; label: string }[] = [
  { section: "general", label: "General" },
  { section: "polish", label: "AI polish" },
  { section: "insertion", label: "Insertion" },
  { section: "privacy", label: "Data & Privacy" },
  { section: "experimental", label: "Experimental" },
  { section: "about", label: "About" },
];

export function Settings() {
  const section = useHub((s) => s.section);
  const settings = useHub((s) => s.settings);
  return (
    <>
      <PageHeader title="Settings" />
      <div role="tablist" aria-label="Settings" className="mb-7 flex gap-6 border-b border-line">
        {TABS.map((tab) => {
          const active = tab.section === section;
          return (
            <button
              key={tab.section}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => useHub.setState({ section: tab.section })}
              className={cn(
                "relative h-9 text-[13px] font-medium transition-colors",
                active ? "text-ink" : "text-ink-soft hover:text-ink",
              )}
            >
              {tab.label}
              {active && (
                <motion.span
                  layoutId="settings-tab"
                  transition={{ type: "spring", stiffness: 520, damping: 40 }}
                  className="absolute inset-x-0 -bottom-px h-[2px] rounded-full bg-ink"
                />
              )}
            </button>
          );
        })}
      </div>
      {settings && (
        <div key={section} role="tabpanel" className="space-y-8 animate-fade-in">
          {section === "general" && <General settings={settings} />}
          {section === "polish" && <Polish settings={settings} />}
          {section === "insertion" && <Insertion settings={settings} />}
          {section === "privacy" && <Privacy settings={settings} />}
          {section === "experimental" && <Experimental settings={settings} />}
          {section === "about" && <About settings={settings} />}
        </div>
      )}
    </>
  );
}
