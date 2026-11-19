import { AnimatePresence, motion } from "motion/react";
import { Check, ChevronDown, Cloud, Mic, Play, Sparkles, Square, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";
import type { Listening_Serialize as Listening, RouteLabel_Serialize as RouteLabel } from "@/lib/bindings";
import { bar } from "./connect";
import { MicBars } from "./MicBars";
import { useBar } from "./store";

const SPRING = { type: "spring", stiffness: 520, damping: 40, mass: 0.8 } as const;

/** The Joona Bar's pill. It grows from a thin resting line into the dictation controls. */
export function Pill() {
  const state = useBar((s) => s.state);
  const resting = useBar((s) => s.resting);
  const hover = useBar((s) => s.pillHover || s.menu !== null);
  const paused = useBar((s) => s.config?.paused ?? false);
  const setPillHover = useBar((s) => s.setPillHover);

  const kind = state.state;
  const open = kind === "resting" && hover;
  const visible = kind !== "resting" || resting || hover;

  let size: { width: number | "auto"; height: number };
  let body: ReactNode;
  switch (kind) {
    case "resting":
      size = open ? { width: "auto", height: 32 } : { width: 40, height: 8 };
      body = open ? <RestingOpen paused={paused} /> : null;
      break;
    case "listening":
      size = { width: "auto", height: 36 };
      body = state.handsFree ? <HandsFree listening={state} /> : <Holding listening={state} />;
      break;
    case "processing":
      size = { width: "auto", height: 36 };
      body = <Processing route={state.route} />;
      break;
    case "done":
      size = { width: 36, height: 36 };
      body = <Done />;
      break;
  }

  const onClick = () => {
    if (kind !== "resting") return;
    if (paused) void bar.setPaused(false);
    else void bar.start();
  };

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key="pill"
          data-hit
          className={cn("flex items-center justify-center p-2.5", kind === "resting" && "cursor-pointer")}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 6, transition: { duration: 0.2 } }}
          onPointerEnter={() => setPillHover(true)}
          onPointerLeave={() => setPillHover(false)}
          onClick={onClick}
          onContextMenu={(e) => {
            e.preventDefault();
            if (kind === "resting") useBar.getState().setMenu("context");
          }}
        >
          <motion.div
            role="status"
            aria-label={describe(kind, paused)}
            className={cn(
              "relative flex items-center justify-center overflow-hidden rounded-full text-white",
              "shadow-[0_1px_2px_rgb(0_0_0/0.18),0_6px_20px_rgb(0_0_0/0.22)]",
              kind === "resting" && !open
                ? "bg-[oklch(0.2_0.01_60/0.72)] ring-1 ring-white/35"
                : "bg-[oklch(0.15_0.008_60)] ring-1 ring-white/12",
              paused && kind === "resting" && !open && "opacity-55",
            )}
            initial={false}
            animate={size}
            transition={SPRING}
          >
            <AnimatePresence mode="popLayout" initial={false}>
              {body && (
                <motion.div
                  key={bodyKey(state)}
                  className="flex shrink-0 items-center whitespace-nowrap"
                  initial={{ opacity: 0, scale: 0.92 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.92, transition: { duration: 0.12 } }}
                  transition={{ duration: 0.16, delay: 0.04 }}
                >
                  {body}
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function bodyKey(state: ReturnType<typeof useBar.getState>["state"]) {
  return state.state === "listening" ? `listening-${state.handsFree}` : state.state;
}

function describe(kind: string, paused: boolean) {
  if (kind === "resting") return paused ? "Joona is paused" : "Joona is ready";
  if (kind === "listening") return "Listening";
  if (kind === "processing") return "Transcribing";
  return "Done";
}

function RestingOpen({ paused }: { paused: boolean }) {
  return (
    <div className="flex items-center gap-1.5 pl-1.5 pr-1.5">
      <span className="grid size-[22px] place-items-center rounded-full bg-white/12">
        {paused ? <Play className="size-3 fill-white" /> : <Mic className="size-3.5" strokeWidth={2.25} />}
      </span>
      <span className="pr-1 text-[12px] font-medium text-white/85">{paused ? "Resume Joona" : "Dictate"}</span>
      {!paused && <LanguageChip />}
    </div>
  );
}

/** The language picker. Hidden when there is only one language to speak and no translation. */
function LanguageChip() {
  const config = useBar((s) => s.config);
  const menu = useBar((s) => s.menu);
  if (!config || (config.languages.length < 2 && !config.output)) return null;
  return (
    <button
      type="button"
      aria-label="Change language"
      className={cn(
        "flex h-[22px] items-center gap-0.5 rounded-full px-2 text-[11px] font-semibold tracking-wide text-white/90",
        menu === "language" ? "bg-white/25" : "bg-white/10 hover:bg-white/20",
      )}
      onClick={(e) => {
        e.stopPropagation();
        useBar.getState().setMenu(menu === "language" ? null : "language");
      }}
    >
      {config.route}
      <ChevronDown className="size-3 opacity-70" strokeWidth={2.5} />
    </button>
  );
}

function Holding({ listening }: { listening: Listening }) {
  return (
    <div className="flex items-center gap-2.5 px-3.5">
      <RouteTags listening={listening} />
      <MicBars />
    </div>
  );
}

function HandsFree({ listening }: { listening: Listening }) {
  return (
    <div className="flex items-center gap-2 px-1.5">
      <RoundButton label="Cancel" onClick={() => void bar.cancel()}>
        <X className="size-3.5" strokeWidth={2.5} />
      </RoundButton>
      <RouteTags listening={listening} />
      <MicBars />
      <Timer endingSoon={listening.endingSoon} />
      <RoundButton label="Stop and insert" tone="stop" onClick={() => void bar.stop()}>
        <Square className="size-2.5 fill-current" strokeWidth={0} />
      </RoundButton>
    </div>
  );
}

/** Command Mode, a translation or a cloud engine, so the user knows what will happen. */
function RouteTags({ listening }: { listening: Listening }) {
  const { mode, route } = listening;
  return (
    <>
      {route.cloud && <CloudBadge name={route.cloud} />}
      {mode === "command" ? (
        <span className="flex items-center gap-1 text-[11px] font-semibold text-accent">
          <Sparkles className="size-3" strokeWidth={2.5} />
          Command
        </span>
      ) : (
        route.crossLanguage && <span className="text-[11px] font-semibold tracking-wide text-white/65">{route.text}</span>
      )}
    </>
  );
}

function CloudBadge({ name }: { name: string }) {
  return (
    <span
      title={`Sent to ${name}`}
      className="flex items-center gap-1 rounded-full bg-cloud/25 px-1.5 py-0.5 text-[10.5px] font-semibold text-[oklch(0.86_0.08_250)]"
    >
      <Cloud className="size-3" strokeWidth={2.5} />
      {name}
    </span>
  );
}

function Timer({ endingSoon }: { endingSoon: boolean }) {
  const startedAt = useBar((s) => s.startedAt);
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(performance.now()), 250);
    return () => window.clearInterval(id);
  }, []);
  const seconds = Math.max(0, Math.floor((now - startedAt) / 1000));
  const text = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  return (
    <span className={cn("tabular min-w-[30px] text-right text-[12px] font-medium", endingSoon ? "text-accent" : "text-white/75")}>
      {text}
    </span>
  );
}

function RoundButton({
  label,
  tone = "plain",
  onClick,
  children,
}: {
  label: string;
  tone?: "plain" | "stop";
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className={cn(
        "grid size-6 place-items-center rounded-full transition-colors",
        tone === "stop" ? "bg-[oklch(0.62_0.2_27)] text-white hover:bg-[oklch(0.67_0.2_27)]" : "bg-white/12 text-white/85 hover:bg-white/22",
      )}
    >
      {children}
    </button>
  );
}

function Processing({ route }: { route: RouteLabel }) {
  return (
    <div className="flex items-center gap-2.5 px-3.5">
      {route.cloud && <CloudBadge name={route.cloud} />}
      <MicBars frozen />
    </div>
  );
}

function Done() {
  return (
    <motion.span initial={{ scale: 0.4 }} animate={{ scale: 1 }} transition={{ type: "spring", stiffness: 600, damping: 22 }}>
      <Check className="size-4 text-[oklch(0.8_0.15_155)]" strokeWidth={3} />
    </motion.span>
  );
}
