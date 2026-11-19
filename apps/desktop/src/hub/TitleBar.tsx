import { getCurrentWindow } from "@tauri-apps/api/window";
import { type ReactNode, useEffect, useState } from "react";
import { cn } from "@/lib/utils";

/** Drag area and Windows-style caption buttons. Closing hides the Hub; Joona stays in the tray. */
export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let gone = false;
    const update = () => void win.isMaximized().then(setMaximized, () => {});
    update();
    void win.onResized(update).then((u) => (gone ? u() : (unlisten = u)));
    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);

  const win = () => getCurrentWindow();
  return (
    <div data-tauri-drag-region className="flex h-10 shrink-0 items-stretch justify-end">
      <CaptionButton label="Minimize" onClick={() => void win().minimize()}>
        <path d="M0.5 5.5h10" />
      </CaptionButton>
      <CaptionButton label={maximized ? "Restore down" : "Maximize"} onClick={() => void win().toggleMaximize()}>
        {maximized ? (
          <>
            <rect x="0.5" y="2.5" width="8" height="8" rx="1" />
            <path d="M2.5 2.5v-1a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-1" />
          </>
        ) : (
          <rect x="0.5" y="0.5" width="10" height="10" rx="1.2" />
        )}
      </CaptionButton>
      <CaptionButton label="Close" close onClick={() => void win().close()}>
        <path d="M0.5 0.5l10 10M10.5 0.5l-10 10" />
      </CaptionButton>
    </div>
  );
}

function CaptionButton({
  label,
  close,
  onClick,
  children,
}: {
  label: string;
  close?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "grid w-[46px] place-items-center text-ink-soft transition-colors duration-100",
        close ? "hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85" : "hover:bg-ink/[0.06] hover:text-ink",
      )}
    >
      <svg viewBox="0 0 11 11" className="size-[10px]" fill="none" stroke="currentColor" strokeWidth="1">
        {children}
      </svg>
    </button>
  );
}
