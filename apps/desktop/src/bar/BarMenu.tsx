import { motion } from "motion/react";
import { Check, EyeOff, Languages, LayoutGrid, LogOut, Pause, Settings2 } from "lucide-react";
import type { ReactNode } from "react";
import { commands } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { bar } from "./connect";
import { useBar } from "./store";

/** The right-click menu, or the language picker when opened from the language chip. */
export function BarMenu() {
  const menu = useBar((s) => s.menu);
  const config = useBar((s) => s.config);
  const close = () => useBar.getState().setMenu(null);
  if (!menu || !config) return null;

  return (
    <motion.div
      key={menu}
      data-hit
      role="menu"
      initial={{ opacity: 0, y: 6, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 0.14, ease: "easeOut" }}
      className="w-60 origin-bottom rounded-xl bg-surface-raised p-1 text-[13px] text-ink shadow-lifted ring-1 ring-black/[0.06] dark:ring-white/10"
      onContextMenu={(e) => e.preventDefault()}
    >
      {menu === "language" ? (
        <>
          <Heading>{config.outputName ? `Speak, then type in ${config.outputName}` : "Language you speak"}</Heading>
          <div className="max-h-[132px] overflow-y-auto">
            {config.languages.length > 1 && (
              <Item checked={config.current === null} onClick={() => void bar.setLanguage(null).then(close)}>
                Detect automatically
              </Item>
            )}
            {config.languages.map((language) => (
              <Item
                key={language.code}
                checked={config.languages.length === 1 || config.current === language.code}
                hint={language.native !== language.name ? language.native : undefined}
                onClick={() => void bar.setLanguage(language.code).then(close)}
              >
                {language.name}
              </Item>
            ))}
          </div>
          <Separator />
          <Item icon={<Settings2 />} onClick={() => void bar.openHub("languages").then(close)}>
            Language settings…
          </Item>
        </>
      ) : (
        <>
          <Item icon={<EyeOff />} onClick={() => void bar.snooze(true).then(close)}>
            Hide for 1 hour
          </Item>
          <Item icon={<Pause />} onClick={() => void bar.setPaused(!config.paused).then(close)}>
            {config.paused ? "Resume Joona" : "Pause Joona"}
          </Item>
          <Item icon={<LayoutGrid />} onClick={() => void bar.openHub().then(close)}>
            Open Joona
          </Item>
          {(config.languages.length > 1 || config.output) && (
            <>
              <Separator />
              <Item icon={<Languages />} hint={config.route} onClick={() => useBar.getState().setMenu("language")}>
                Change language
              </Item>
            </>
          )}
          <Separator />
          <Item icon={<LogOut />} onClick={() => void commands.quitApp()}>
            Quit Joona
          </Item>
        </>
      )}
    </motion.div>
  );
}

function Item({
  icon,
  checked,
  hint,
  onClick,
  children,
}: {
  icon?: ReactNode;
  checked?: boolean;
  hint?: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      onClick={onClick}
      className="flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left transition-colors hover:bg-fill"
    >
      <span className={cn("grid size-4 shrink-0 place-items-center text-ink-soft [&>svg]:size-4", checked === false && "invisible")}>
        {checked ? <Check strokeWidth={2.5} className="text-ink" /> : icon}
      </span>
      <span className="flex-1 truncate">{children}</span>
      {hint && <span className="truncate text-[12px] text-ink-faint">{hint}</span>}
    </button>
  );
}

function Heading({ children }: { children: ReactNode }) {
  return <p className="px-2.5 pb-1 pt-1.5 text-[11px] font-medium text-ink-faint">{children}</p>;
}

function Separator() {
  return <div role="separator" className="mx-2 my-1 h-px bg-line" />;
}
