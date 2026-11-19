import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** A labelled control in a dialog, with an optional hint and error below it. */
export function Field({
  id,
  label,
  aside,
  hint,
  error,
  className,
  children,
}: {
  id: string;
  label: string;
  /** Shown at the end of the label line, such as a character count. */
  aside?: ReactNode;
  hint?: ReactNode;
  error?: string | null;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("mt-5", className)}>
      <div className="flex items-baseline justify-between gap-3">
        <label htmlFor={id} className="text-[13px] font-medium text-ink">
          {label}
        </label>
        {aside}
      </div>
      <div className="mt-1.5">{children}</div>
      {error && (
        <p role="alert" className="mt-2 text-[12.5px] text-danger">
          {error}
        </p>
      )}
      {hint && <p className="mt-2 text-[12px] leading-relaxed text-ink-faint">{hint}</p>}
    </div>
  );
}
