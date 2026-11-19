import { openUrl } from "@tauri-apps/plugin-opener";
import type { ReactNode } from "react";

/** A link in running text that opens in the browser. */
export function TextLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={() => void openUrl(href)}
      className="font-medium text-ink underline decoration-line-strong underline-offset-2 hover:decoration-ink"
    >
      {children}
    </button>
  );
}
