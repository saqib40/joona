import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  "inline-flex shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap font-medium transition-[background-color,color,box-shadow,opacity] disabled:pointer-events-none disabled:opacity-45 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary: "bg-primary text-primary-ink shadow-soft hover:bg-primary/85",
        secondary:
          "bg-surface-raised text-ink shadow-[0_1px_1px_hsl(var(--shadow-color)/0.05)] ring-1 ring-line-strong/80 hover:bg-fill",
        ghost: "text-ink-soft hover:bg-fill hover:text-ink",
        accent: "bg-accent text-[oklch(0.22_0.03_60)] shadow-soft hover:brightness-105",
        danger: "bg-danger text-white shadow-soft hover:bg-danger/90",
        quiet: "text-danger hover:bg-danger-soft",
      },
      size: {
        sm: "h-7 rounded-md px-2.5 text-[12px] [&_svg]:size-3.5",
        md: "h-8 rounded-lg px-3.5 text-[13px] [&_svg]:size-4",
        lg: "h-10 rounded-xl px-5 text-sm [&_svg]:size-4",
        icon: "size-8 rounded-lg [&_svg]:size-4",
        "icon-sm": "size-7 rounded-md [&_svg]:size-3.5",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> & VariantProps<typeof buttonVariants>;

export function Button({ className, variant, size, type = "button", ...props }: ButtonProps) {
  return <button type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
