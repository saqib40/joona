import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Keys } from "@/components/ui/kbd";
import { commands, events, type Shortcut_Serialize as Shortcut } from "@/lib/bindings";
import { cn, isMac } from "@/lib/utils";
import { reservedBy, sameShortcut, shortcutKeys, shortcutLabel } from "../shortcut";

/**
 * Shows a shortcut and records a new one. While recording, Joona's keyboard
 * hook takes every key, so Win doesn't open Start and the old shortcut
 * doesn't start a dictation.
 */
export function ShortcutField({
  value,
  onChange,
  optional,
  taken,
  label,
}: {
  value: Shortcut | null;
  onChange: (shortcut: Shortcut | null) => void;
  /** Can be removed. */
  optional?: boolean;
  /** Other shortcuts Joona uses, which this one mustn't repeat. */
  taken?: { shortcut: Shortcut | null; name: string }[];
  label: string;
}) {
  const [recording, setRecording] = useState(false);
  const [preview, setPreview] = useState<string[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const unlisten = useRef<(() => void) | null>(null);

  const finish = () => {
    unlisten.current?.();
    unlisten.current = null;
    setRecording(false);
    setPreview(null);
  };

  const cancel = () => {
    void commands.shortcutRecordCancel();
    finish();
  };

  const start = async () => {
    setProblem(null);
    setPreview(null);
    unlisten.current?.();
    unlisten.current = await events.shortcutRecording.listen(({ payload }) => {
      if (payload.kind === "preview") {
        setPreview(shortcutKeys(payload.shortcut));
        return;
      }
      finish();
      if (payload.kind === "cancelled") return;
      const shortcut = payload.shortcut;
      const clash = taken?.find((t) => sameShortcut(t.shortcut, shortcut));
      const reserved = reservedBy(shortcut);
      if (!payload.trigger) {
        setProblem(
          isMac
            ? `${payload.label} on its own would get in the way of typing. Add Cmd, Option, Ctrl or Shift, or use a key like F8.`
            : `${payload.label} on its own would get in the way of typing. Add Ctrl, Alt, Shift or Win, or use a key like F8.`,
        );
      } else if (reserved) {
        setProblem(`${reserved}, so pick another.`);
      } else if (clash) {
        setProblem(`${shortcutLabel(shortcut)} is already the ${clash.name} shortcut.`);
      } else {
        onChange(shortcut);
      }
    });
    setRecording(true);
    try {
      await commands.shortcutRecord();
    } catch (e) {
      finish();
      setProblem(String(e));
    }
  };

  useEffect(() => {
    if (!recording) return;
    // Keys go to the recorder whichever window is in front, so leaving the Hub stops it.
    window.addEventListener("blur", cancel);
    return () => window.removeEventListener("blur", cancel);
  });

  useEffect(
    () => () => {
      if (unlisten.current) {
        void commands.shortcutRecordCancel();
        unlisten.current();
      }
    },
    [],
  );

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex items-center gap-1.5">
        <button
          type="button"
          aria-label={recording ? `Recording ${label}. Press Esc to cancel.` : `Change ${label}`}
          onClick={() => (recording ? cancel() : void start())}
          className={cn(
            "flex h-9 min-w-[156px] items-center justify-center gap-1 rounded-lg px-3 transition-[background-color,box-shadow]",
            recording ? "bg-accent-soft ring-2 ring-accent" : "bg-fill ring-1 ring-line hover:bg-fill-strong",
          )}
        >
          {recording ? (
            preview?.length ? (
              <Keys keys={preview} />
            ) : (
              <span className="text-[12.5px] font-medium text-accent-ink">Press the keys…</span>
            )
          ) : value ? (
            <Keys keys={shortcutKeys(value)} />
          ) : (
            <span className="text-[12.5px] text-ink-faint">Not set</span>
          )}
        </button>
        {optional && value && !recording && (
          <Button size="icon-sm" variant="ghost" aria-label={`Remove ${label}`} onClick={() => onChange(null)}>
            <X />
          </Button>
        )}
      </div>
      {recording && <p className="text-[11.5px] text-ink-faint">Let go to save · Esc to cancel</p>}
      {problem && !recording && (
        <p role="alert" className="max-w-[280px] text-right text-[11.5px] leading-snug text-danger">
          {problem}
        </p>
      )}
    </div>
  );
}
