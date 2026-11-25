import { Mic, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { commands, events } from "@/lib/bindings";
import { cn } from "@/lib/utils";

/** The test stops on its own, like the backend's own limit. */
const LIMIT_MS = 60_000;

/**
 * Opens the microphone and shows its level, without dictating. `large` is the
 * onboarding version, which starts by itself.
 */
export function MicTest({
  device,
  large,
  onHeard,
}: {
  device: string | null;
  large?: boolean;
  /** Called once the level shows someone talking. */
  onHeard?: () => void;
}) {
  const segments = large ? 36 : 22;
  const [running, setRunning] = useState(false);
  const [starting, setStarting] = useState(false);
  const [name, setName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [heard, setHeard] = useState(false);
  const meter = useRef<HTMLDivElement>(null);
  const mounted = useRef(true);
  const attempt = useRef(0);

  const paint = (level: number) => {
    const lit = Math.round(level * segments);
    meter.current?.childNodes.forEach((node, i) => {
      (node as HTMLElement).dataset.on = String(i < lit);
    });
    if (level > 0.35) setHeard(true);
  };

  const stop = () => {
    attempt.current += 1;
    void commands.micTestStop();
    setRunning(false);
    paint(0);
  };

  const start = async () => {
    const id = ++attempt.current;
    setError(null);
    setHeard(false);
    setStarting(true);
    try {
      const deviceName = await commands.micTestStart(device);
      if (!mounted.current || attempt.current !== id) {
        void commands.micTestStop();
        return;
      }
      setName(deviceName);
      setRunning(true);
    } catch (e) {
      if (mounted.current && attempt.current === id) {
        setError(String(e));
      }
    } finally {
      if (mounted.current && attempt.current === id) {
        setStarting(false);
      }
    }
  };

  useEffect(() => {
    if (!running) return;
    let unlisten: (() => void) | undefined;
    let gone = false;
    void events.micTestLevel.listen((e) => paint(e.payload ?? 0)).then((u) => (gone ? u() : (unlisten = u)));
    const timer = window.setTimeout(stop, LIMIT_MS);
    return () => {
      gone = true;
      unlisten?.();
      window.clearTimeout(timer);
    };
  }, [running]);

  useEffect(() => {
    if (heard) onHeard?.();
  }, [heard]);

  // A different microphone needs a new test; leaving the page ends it.
  useEffect(() => {
    if (large) void start();
    else if (running) stop();
  }, [device]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      attempt.current += 1;
      void commands.micTestStop();
    };
  }, []);

  const bars = (
    <div ref={meter} className={cn("flex items-center", large ? "h-12 gap-[5px]" : "h-4 gap-[3px]")} aria-hidden>
      {Array.from({ length: segments }, (_, i) => (
        <span
          key={i}
          data-on="false"
          className={cn(
            "h-full rounded-full bg-fill-strong transition-colors duration-75 data-[on=true]:bg-accent",
            large ? "w-[7px]" : "w-[4px]",
          )}
        />
      ))}
    </div>
  );

  const status = error ? (
    <p role="alert" className={cn("text-[11.5px] leading-snug text-danger", !large && "max-w-[300px] text-right")}>
      {error}
    </p>
  ) : running ? (
    <p className={cn("text-ink-faint", large ? "text-[13px]" : "text-[11.5px]")}>
      {heard ? "Sounds good." : "Say something."} Listening on {name}
    </p>
  ) : null;

  if (large) {
    return (
      <div className="flex flex-col items-center gap-4">
        {bars}
        {status ?? <p className="text-[13px] text-ink-faint">{starting ? "Opening your microphone…" : "\u00a0"}</p>}
        {!running && !starting && (
          <Button size="sm" onClick={() => void start()}>
            <Mic />
            Listen again
          </Button>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex items-center gap-3">
        {bars}
        {running ? (
          <Button size="sm" onClick={stop}>
            <Square />
            Stop
          </Button>
        ) : (
          <Button size="sm" disabled={starting} onClick={() => void start()}>
            <Mic />
            Test
          </Button>
        )}
      </div>
      {status}
    </div>
  );
}
