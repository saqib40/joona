import { cn } from "@/lib/utils";

const SIZES = {
  md: "h-[22px] min-w-[22px] rounded-md px-1.5 text-[11.5px] shadow-[inset_0_-1.5px_0_var(--line-strong)]",
  lg: "h-11 min-w-11 rounded-xl px-4 text-[17px] shadow-[inset_0_-3px_0_var(--line-strong)]",
};

export function Kbd({ children, className, size = "md" }: { children: string; className?: string; size?: keyof typeof SIZES }) {
  return (
    <kbd
      className={cn(
        "inline-flex items-center justify-center bg-surface-raised font-sans font-semibold leading-none text-ink ring-1 ring-line-strong/80",
        SIZES[size],
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/** A key combination, for example Ctrl + Win. */
export function Keys({ keys, className, size }: { keys: string[]; className?: string; size?: keyof typeof SIZES }) {
  return (
    <span className={cn("inline-flex items-center align-middle", size === "lg" ? "gap-2" : "gap-1", className)}>
      {keys.map((key, i) => (
        <Kbd key={`${key}-${i}`} size={size}>
          {key}
        </Kbd>
      ))}
    </span>
  );
}
