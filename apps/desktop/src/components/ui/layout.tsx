import type { LucideIcon } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

export function PageHeader({
  title,
  description,
  actions,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="flex items-end justify-between gap-6 pb-7 pt-4">
      <div className="min-w-0">
        <h1 className="font-display text-[38px] leading-[1.05] tracking-[-0.015em] text-ink">{title}</h1>
        {description && <p className="mt-2 max-w-[560px] text-[13.5px] leading-relaxed text-ink-soft">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2 pb-1">{actions}</div>}
    </header>
  );
}

/** A titled block of settings rows on one card. */
export function Group({
  title,
  description,
  action,
  children,
  className,
}: {
  title?: string;
  description?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("space-y-2.5", className)}>
      {(title || action) && (
        <div className="flex items-end justify-between gap-4 px-1">
          <div>
            {title && <h2 className="text-[13px] font-semibold text-ink">{title}</h2>}
            {description && <p className="mt-0.5 text-[12.5px] text-ink-soft">{description}</p>}
          </div>
          {action}
        </div>
      )}
      <div className="divide-y divide-line overflow-hidden rounded-xl bg-surface-raised shadow-soft ring-1 ring-line">
        {children}
      </div>
    </section>
  );
}

export function Row({
  title,
  description,
  children,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex min-h-[58px] items-center justify-between gap-8 px-4 py-3", className)}>
      <div className="min-w-0">
        <div className="text-[13.5px] font-medium text-ink">{title}</div>
        {description && <div className="mt-0.5 text-[12.5px] leading-snug text-ink-soft">{description}</div>}
      </div>
      {children && <div className="flex shrink-0 items-center gap-2">{children}</div>}
    </div>
  );
}

export function Card({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("rounded-xl bg-surface-raised shadow-soft ring-1 ring-line", className)} {...props} />;
}

export function EmptyState({
  icon: Icon,
  title,
  children,
  action,
  className,
}: {
  icon: LucideIcon;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center px-6 py-14 text-center", className)}>
      <div className="grid size-11 place-items-center rounded-2xl bg-fill text-ink-soft">
        <Icon className="size-5" strokeWidth={1.75} />
      </div>
      <h3 className="mt-4 font-display text-[24px] leading-tight text-ink">{title}</h3>
      {children && <div className="mt-1.5 max-w-[400px] text-[13px] leading-relaxed text-ink-soft">{children}</div>}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}

/** Five dots, for speed and accuracy. */
export function Dots({ value, label }: { value: number; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5" aria-label={`${label}: ${value} of 5`}>
      <span className="text-ink-faint">{label}</span>
      <span className="inline-flex gap-[3px]">
        {[1, 2, 3, 4, 5].map((i) => (
          <span key={i} className={cn("size-[5px] rounded-full", i <= value ? "bg-ink" : "bg-fill-strong")} />
        ))}
      </span>
    </span>
  );
}

export function ProgressBar({ value, className }: { value: number | null; className?: string }) {
  return (
    <div className={cn("relative h-1.5 overflow-hidden rounded-full bg-fill-strong", className)}>
      {value === null ? (
        <div className="absolute inset-0 animate-shimmer bg-[linear-gradient(90deg,transparent,var(--accent),transparent)] bg-[length:200%_100%]" />
      ) : (
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-300 ease-out"
          style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%` }}
        />
      )}
    </div>
  );
}
