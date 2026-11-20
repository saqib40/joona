import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { isMac } from "@/lib/utils";
import { BarMenu } from "./BarMenu";
import { connect } from "./connect";
import { Pill } from "./Pill";
import { useHitRegions } from "./regions";
import { useBar } from "./store";
import { Toasts } from "./Toasts";

/** How long the cursor rests on the pill before the hint shows. */
const HINT_DELAY_MS = 450;

/**
 * The Joona Bar. The window is transparent and click-through except where
 * elements are marked `data-hit`; everything is anchored to its bottom edge.
 */
export function Bar() {
  useEffect(() => {
    void connect();
  }, []);
  useHitRegions();
  useTheme();
  useCloseMenuWhenLeft();
  const menu = useBar((s) => s.menu);

  return (
    <div className="fixed inset-0 flex flex-col items-center justify-end pb-1.5">
      <div className="mb-1 flex w-full flex-col items-center justify-end px-4">
        {menu ? <BarMenu /> : <Toasts />}
      </div>
      <div className="relative flex justify-center">
        <Hint />
        <Pill />
      </div>
    </div>
  );
}

function Hint() {
  const show = useBar((s) => (s.hover || s.pillHover) && s.state.state === "resting" && !s.menu && s.toasts.length === 0);
  const shortcut = useBar((s) => s.config?.shortcut ?? (isMac ? "Ctrl+Cmd" : "Ctrl+Win"));
  const paused = useBar((s) => s.config?.paused ?? false);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (!show) {
      setReady(false);
      return;
    }
    const id = window.setTimeout(() => setReady(true), HINT_DELAY_MS);
    return () => window.clearTimeout(id);
  }, [show]);

  return (
    <AnimatePresence>
      {show && ready && (
        <motion.div
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, transition: { duration: 0.1 } }}
          transition={{ duration: 0.14 }}
          className="pointer-events-none absolute bottom-full mb-0.5 whitespace-nowrap rounded-lg bg-[oklch(0.15_0.008_60)] px-2.5 py-1.5 text-[11.5px] text-white/80 shadow-lifted"
        >
          {paused ? (
            "Joona is paused. Click to resume."
          ) : (
            <>
              Hold <Key>{shortcut}</Key> to dictate, or click for hands-free
            </>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function Key({ children }: { children: string }) {
  return <kbd className="rounded bg-white/14 px-1 py-px font-sans text-[11px] font-semibold text-white">{children}</kbd>;
}

/** Follows the theme chosen in settings, or Windows' light or dark mode. */
function useTheme() {
  const theme = useBar((s) => s.config?.theme ?? "system");
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () =>
      document.documentElement.classList.toggle("dark", theme === "dark" || (theme === "system" && media.matches));
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

/**
 * The webview stops getting mouse events once the cursor leaves the bar, because
 * the window turns click-through again, so menus close on Rust's hover event instead.
 */
function useCloseMenuWhenLeft() {
  const hover = useBar((s) => s.hover);
  const menu = useBar((s) => s.menu);
  useEffect(() => {
    if (hover || !menu) return;
    const id = window.setTimeout(() => useBar.getState().setMenu(null), 600);
    return () => window.clearTimeout(id);
  }, [hover, menu]);
}
