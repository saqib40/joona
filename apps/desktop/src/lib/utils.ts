import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const isMac =
  (typeof process !== "undefined" && process.platform === "darwin") ||
  (typeof navigator !== "undefined" &&
    (/Macintosh|MacIntel|MacPPC|Mac68K|Mac/i.test(navigator.userAgent) ||
      /Mac/i.test(navigator.platform) ||
      (navigator as any).userAgentData?.platform === "macOS"));
