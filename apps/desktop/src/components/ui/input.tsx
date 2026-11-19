import { Search, X } from "lucide-react";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Input({ className, ...props }: ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "h-8 w-full rounded-lg bg-surface-raised px-3 text-[13px] text-ink shadow-[0_1px_1px_hsl(var(--shadow-color)/0.05)] ring-1 ring-line-strong/80 transition-shadow placeholder:text-ink-faint focus:outline-none focus:ring-2 focus:ring-ring",
        className,
      )}
      {...props}
    />
  );
}

export function Textarea({ className, ...props }: ComponentProps<"textarea">) {
  return (
    <textarea
      className={cn(
        "block w-full resize-none rounded-lg bg-surface-raised px-3 py-2 text-[13px] leading-relaxed text-ink shadow-[0_1px_1px_hsl(var(--shadow-color)/0.05)] ring-1 ring-line-strong/80 transition-shadow placeholder:text-ink-faint focus:outline-none focus:ring-2 focus:ring-ring",
        className,
      )}
      {...props}
    />
  );
}

export function SearchInput({
  value,
  onChange,
  placeholder,
  className,
  autoFocus,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  className?: string;
  autoFocus?: boolean;
}) {
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-ink-faint" />
      <Input
        type="search"
        value={value}
        autoFocus={autoFocus}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && onChange("")}
        className="pl-8 pr-8 [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => onChange("")}
          className="absolute right-1.5 top-1/2 grid size-5 -translate-y-1/2 place-items-center rounded-full text-ink-faint hover:bg-fill hover:text-ink"
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}
