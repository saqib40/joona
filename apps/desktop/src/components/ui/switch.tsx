import { Switch as SwitchPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export function Switch({ className, ...props }: ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      className={cn(
        "inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full p-[2px] ring-1 ring-inset ring-black/5 transition-colors disabled:opacity-45 data-[state=checked]:bg-primary data-[state=unchecked]:bg-fill-strong dark:ring-white/10",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className="pointer-events-none block size-[18px] rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.22)] transition-transform duration-150 data-[state=checked]:translate-x-4 dark:data-[state=checked]:bg-primary-ink dark:data-[state=unchecked]:bg-ink-soft" />
    </SwitchPrimitive.Root>
  );
}
