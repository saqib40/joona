import { AlertDialog } from "radix-ui";
import type { ReactNode } from "react";
import { buttonVariants } from "./button";

/**
 * Asks before something that can't be undone. Either `children` is the button
 * that opens it, or `open` controls it, for example from a menu item that
 * closes its menu first.
 */
export function Confirm({
  title,
  description,
  confirm,
  onConfirm,
  children,
  open,
  onOpenChange,
}: {
  title: string;
  description: ReactNode;
  confirm: string;
  onConfirm: () => void;
  children?: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <AlertDialog.Root open={open} onOpenChange={onOpenChange}>
      {children && <AlertDialog.Trigger asChild>{children}</AlertDialog.Trigger>}
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/25 animate-fade-in dark:bg-black/50" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-50 w-[min(420px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl bg-surface-raised p-6 shadow-lifted ring-1 ring-black/[0.07] animate-fade-in dark:ring-white/10">
          <AlertDialog.Title className="font-display text-[26px] leading-tight text-ink">{title}</AlertDialog.Title>
          <AlertDialog.Description className="mt-2 text-[13.5px] leading-relaxed text-ink-soft">
            {description}
          </AlertDialog.Description>
          <div className="mt-6 flex justify-end gap-2">
            <AlertDialog.Cancel className={buttonVariants({ variant: "secondary" })}>Cancel</AlertDialog.Cancel>
            <AlertDialog.Action className={buttonVariants({ variant: "danger" })} onClick={onConfirm}>
              {confirm}
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
