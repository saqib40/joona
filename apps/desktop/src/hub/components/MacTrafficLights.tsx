import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";

export function MacTrafficLights({ className }: { className?: string }) {
  const [hovered, setHovered] = useState(false);
  const [altPressed, setAltPressed] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Alt" || e.altKey) setAltPressed(true);
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Alt" || !e.altKey) setAltPressed(false);
    };
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    try {
      const win = getCurrentWindow();
      void win.isFullscreen().then(setFullscreen).catch(() => {});
      void win
        .onResized(() => {
          void win.isFullscreen().then(setFullscreen).catch(() => {});
        })
        .then((u) => {
          unlisten = u;
        })
        .catch(() => {});
    } catch {}
    return () => unlisten?.();
  }, []);

  const handleGreen = async (e: React.MouseEvent) => {
    const win = getCurrentWindow();
    if (e.altKey || altPressed) {
      // Option + Click = Zoom (Maximize without fullscreen)
      await win.toggleMaximize();
    } else {
      // Normal click = Fullscreen (occupies the whole screen)
      try {
        const isFs = await win.isFullscreen();
        await win.setFullscreen(!isFs);
        setFullscreen(!isFs);
      } catch {
        try {
          await win.setSimpleFullscreen(!fullscreen);
          setFullscreen(!fullscreen);
        } catch {
          await win.toggleMaximize();
        }
      }
    }
  };

  const win = () => getCurrentWindow();

  return (
    <div
      data-tauri-drag-region
      className={cn("flex items-center gap-2", className)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <button
        type="button"
        aria-label="Close"
        onClick={() => void win().close()}
        className="relative flex h-3 w-3 items-center justify-center rounded-full border border-[#e0443e] bg-[#ff5f56] active:brightness-90"
      >
        <svg
          viewBox="0 0 10 10"
          className={cn(
            "h-1.5 w-1.5 stroke-[#4d0000] stroke-[1.5] transition-opacity",
            hovered ? "opacity-100" : "opacity-0"
          )}
        >
          <path d="M2.5 2.5l5 5M7.5 2.5l-5 5" />
        </svg>
      </button>
      <button
        type="button"
        aria-label="Minimize"
        onClick={() => void win().minimize()}
        className="relative flex h-3 w-3 items-center justify-center rounded-full border border-[#dea123] bg-[#ffbd2e] active:brightness-90"
      >
        <svg
          viewBox="0 0 10 10"
          className={cn(
            "h-1.5 w-1.5 stroke-[#5e3d00] stroke-[1.5] transition-opacity",
            hovered ? "opacity-100" : "opacity-0"
          )}
        >
          <path d="M2 5h6" />
        </svg>
      </button>
      <button
        type="button"
        aria-label={altPressed ? "Zoom" : fullscreen ? "Exit Fullscreen" : "Enter Fullscreen"}
        onClick={(e) => void handleGreen(e)}
        className="relative flex h-3 w-3 items-center justify-center rounded-full border border-[#1aab29] bg-[#27c93f] active:brightness-90"
      >
        <svg
          viewBox="0 0 10 10"
          className={cn(
            "h-1.5 w-1.5 stroke-[#004d0d] stroke-[1.5] transition-opacity",
            hovered ? "opacity-100" : "opacity-0"
          )}
        >
          {altPressed ? (
            <path d="M5 2v6M2 5h6" />
          ) : fullscreen ? (
            <path d="M4.5 2.5v2H2.5M4.5 4.5L1.5 1.5M5.5 7.5v-2h2M5.5 5.5l3 3" />
          ) : (
            <path d="M2 4.5V2h2.5M2 2l3 3M8 5.5V8H5.5M8 8l-3-3" />
          )}
        </svg>
      </button>
    </div>
  );
}
