import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { Tooltip } from "./tooltip";

/** A small icon button among the actions that appear when hovering a list row. */
export function RowAction({
  icon: Icon,
  label,
  onClick,
  danger,
  active,
}: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
  danger?: boolean;
  active?: boolean;
}) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        onClick={onClick}
        className={cn(
          "grid size-7 place-items-center rounded-md text-ink-soft transition-colors hover:bg-fill hover:text-ink",
          danger && "hover:bg-danger-soft hover:text-danger",
          active && "bg-fill text-ink",
        )}
      >
        <Icon className="size-3.5" strokeWidth={2} />
      </button>
    </Tooltip>
  );
}

/** Where the row actions sit: top right, shown on hover or keyboard focus. */
export const ROW_ACTIONS =
  "absolute right-3 top-2.5 flex gap-0.5 rounded-lg bg-surface-raised p-0.5 opacity-0 shadow-soft ring-1 ring-line transition-opacity group-focus-within:opacity-100 group-hover:opacity-100";
