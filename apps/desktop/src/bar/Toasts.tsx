import { AnimatePresence, motion } from "motion/react";
import { CircleCheck, CircleX, Info, TriangleAlert, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import type { Tone } from "@/lib/bindings";
import { bar } from "./connect";
import { useBar, type Toast } from "./store";

/** Toasts stack above the pill, newest at the bottom, nearest the pill. */
export function Toasts() {
  const toasts = useBar((s) => s.toasts);
  const hover = useBar((s) => s.hover);
  return (
    <div className="flex w-full flex-col items-center gap-2">
      <AnimatePresence initial={false}>
        {toasts.map((toast) => (
          <ToastCard key={toast.id} toast={toast} paused={hover} />
        ))}
      </AnimatePresence>
    </div>
  );
}

function ToastCard({ toast, paused }: { toast: Toast; paused: boolean }) {
  const remove = useBar((s) => s.removeToast);
  const remaining = useRef(toast.timeoutMs);

  // The countdown stops while the cursor is over the bar, so a toast can't vanish under it.
  useEffect(() => {
    if (paused || toast.timeoutMs === 0) return;
    const started = performance.now();
    const id = window.setTimeout(() => remove(toast.id), remaining.current);
    return () => {
      window.clearTimeout(id);
      remaining.current = Math.max(800, remaining.current - (performance.now() - started));
    };
  }, [paused, remove, toast.id, toast.timeoutMs]);

  return (
    <motion.div
      layout
      data-hit
      role={toast.tone === "error" ? "alert" : "status"}
      initial={{ opacity: 0, y: 10, scale: 0.97 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97, transition: { duration: 0.15 } }}
      transition={{ type: "spring", stiffness: 520, damping: 38 }}
      className="flex w-[380px] items-start gap-2.5 rounded-2xl bg-surface-raised px-3 py-2.5 text-ink shadow-lifted ring-1 ring-black/[0.06] dark:ring-white/10"
    >
      <ToneIcon tone={toast.tone} />
      <div className="min-w-0 flex-1 py-px">
        <p className="text-[13px] font-semibold leading-5">{toast.title}</p>
        {toast.detail && <p className="text-[12px] leading-[18px] text-ink-soft">{toast.detail}</p>}
        {toast.actions.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {toast.actions.map((button, i) => (
              <button
                key={button.action}
                type="button"
                className={cn(
                  "h-7 rounded-full px-3 text-[12px] font-medium transition-colors",
                  i === 0 ? "bg-primary text-primary-ink hover:opacity-90" : "bg-fill-strong text-ink hover:bg-line",
                )}
                onClick={() => {
                  void bar.toastAction(toast.id, button.action);
                  remove(toast.id);
                }}
              >
                {button.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <button
        type="button"
        aria-label="Dismiss"
        className="-mr-1 grid size-6 shrink-0 place-items-center rounded-full text-ink-faint transition-colors hover:bg-fill hover:text-ink"
        onClick={() => remove(toast.id)}
      >
        <X className="size-3.5" strokeWidth={2.25} />
      </button>
    </motion.div>
  );
}

function ToneIcon({ tone }: { tone: Tone }) {
  const props = { className: "mt-0.5 size-4 shrink-0", strokeWidth: 2.25 };
  switch (tone) {
    case "success":
      return <CircleCheck {...props} className={cn(props.className, "text-success")} />;
    case "warning":
      return <TriangleAlert {...props} className={cn(props.className, "text-accent-ink")} />;
    case "error":
      return <CircleX {...props} className={cn(props.className, "text-danger")} />;
    default:
      return <Info {...props} className={cn(props.className, "text-ink-soft")} />;
  }
}
