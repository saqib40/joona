import { cva, type VariantProps } from "class-variance-authority";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex h-5 shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2 text-[11px] font-medium leading-none [&_svg]:size-3",
  {
    variants: {
      tone: {
        neutral: "bg-fill text-ink-soft",
        accent: "bg-accent-soft text-accent-ink",
        success: "bg-success-soft text-success",
        cloud: "bg-cloud-soft text-cloud",
        danger: "bg-danger-soft text-danger",
        ink: "bg-primary text-primary-ink",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export function Badge({
  tone,
  className,
  children,
}: VariantProps<typeof badgeVariants> & { className?: string; children: ReactNode }) {
  return <span className={cn(badgeVariants({ tone }), className)}>{children}</span>;
}
