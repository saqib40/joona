import { Check, ChevronDown } from "lucide-react";
import { Select as SelectPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

export interface Option<T extends string> {
  value: T;
  label: string;
  hint?: string;
  disabled?: boolean;
}

/** Radix Select needs non-empty values, so callers map "none" to a sentinel string. */
export function Select<T extends string>({
  value,
  onChange,
  options,
  placeholder,
  disabled,
  className,
  label,
}: {
  value: T | undefined;
  onChange: (value: T) => void;
  options: Option<T>[];
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  /** Accessible name when no visible label points at the control. */
  label?: string;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onChange(v as T)} disabled={disabled}>
      <SelectPrimitive.Trigger
        aria-label={label}
        className={cn(
          "inline-flex h-8 min-w-36 max-w-64 items-center justify-between gap-2 rounded-lg bg-surface-raised px-3 text-[13px] text-ink shadow-[0_1px_1px_hsl(var(--shadow-color)/0.05)] ring-1 ring-line-strong/80 transition-colors hover:bg-fill disabled:opacity-45 data-[placeholder]:text-ink-faint",
          className,
        )}
      >
        <span className="truncate">
          <SelectPrimitive.Value placeholder={placeholder} />
        </span>
        <SelectPrimitive.Icon>
          <ChevronDown className="size-3.5 shrink-0 text-ink-faint" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={6}
          align="end"
          className="z-50 max-h-[min(340px,var(--radix-select-content-available-height))] min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-xl bg-surface-raised p-1 text-[13px] text-ink shadow-lifted ring-1 ring-black/[0.07] animate-fade-in dark:ring-white/10"
        >
          <SelectPrimitive.Viewport>
            {options.map((option) => (
              <SelectPrimitive.Item
                key={option.value}
                value={option.value}
                disabled={option.disabled}
                className="relative flex h-8 cursor-default select-none items-center gap-3 rounded-lg pl-8 pr-3 outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-fill"
              >
                <SelectPrimitive.ItemIndicator className="absolute left-2.5 grid place-items-center">
                  <Check className="size-3.5" strokeWidth={2.5} />
                </SelectPrimitive.ItemIndicator>
                <SelectPrimitive.ItemText>{option.label}</SelectPrimitive.ItemText>
                {option.hint && <span className="ml-auto text-[12px] text-ink-faint">{option.hint}</span>}
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
