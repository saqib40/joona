import { X } from "lucide-react";
import { Dialog as Primitive } from "radix-ui";
import type { ReactNode } from "react";

/** A sheet over the page for a short task, such as adding an API key. */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: ReactNode;
  children: ReactNode;
}) {
  return (
    <Primitive.Root open={open} onOpenChange={onOpenChange}>
      <Primitive.Portal>
        <Primitive.Overlay className="fixed inset-0 z-50 bg-black/25 animate-fade-in dark:bg-black/50" />
        <Primitive.Content className="fixed left-1/2 top-1/2 z-50 max-h-[calc(100vh-48px)] w-[min(480px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl bg-surface-raised p-6 shadow-lifted ring-1 ring-black/[0.07] animate-fade-in dark:ring-white/10">
          <Primitive.Title className="pr-8 font-display text-[26px] leading-tight text-ink">{title}</Primitive.Title>
          <Primitive.Description className="mt-2 text-[13.5px] leading-relaxed text-ink-soft">
            {description}
          </Primitive.Description>
          {children}
          <Primitive.Close
            aria-label="Close"
            className="absolute right-4 top-4 grid size-7 place-items-center rounded-md text-ink-faint transition-colors hover:bg-fill hover:text-ink"
          >
            <X className="size-4" />
          </Primitive.Close>
        </Primitive.Content>
      </Primitive.Portal>
    </Primitive.Root>
  );
}
