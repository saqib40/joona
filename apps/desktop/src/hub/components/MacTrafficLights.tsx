import { getCurrentWindow } from "@tauri-apps/api/window";
import { useState } from "react";
import { cn } from "@/lib/utils";

export function MacTrafficLights({ className }: { className?: string }) {
  const [hovered, setHovered] = useState(false);
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
        aria-label="Zoom"
        onClick={() => void win().toggleMaximize()}
        className="relative flex h-3 w-3 items-center justify-center rounded-full border border-[#1aab29] bg-[#27c93f] active:brightness-90"
      >
        <svg
          viewBox="0 0 10 10"
          className={cn(
            "h-1.5 w-1.5 stroke-[#004d0d] stroke-[1.5] transition-opacity",
            hovered ? "opacity-100" : "opacity-0"
          )}
        >
          <path d="M2.5 2.5l5 5M2.5 2.5h3.5M2.5 2.5v3.5M7.5 7.5l-5-5M7.5 7.5h-3.5M7.5 7.5v-3.5" />
        </svg>
      </button>
    </div>
  );
}
