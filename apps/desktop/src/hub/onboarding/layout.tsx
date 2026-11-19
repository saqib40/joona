import { ArrowLeft, ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";

export interface StepProps {
  next: () => void;
  back: () => void;
}

/** A step's title, body and footer. The footer stays in view when the body scrolls. */
export function StepLayout({
  title,
  description,
  children,
  footer,
}: {
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer: ReactNode;
}) {
  return (
    <>
      <div className="pt-4">
        <h1 className="font-display text-[42px] leading-[1.04] tracking-[-0.015em] text-ink">{title}</h1>
        {description && (
          <div className="mt-2.5 max-w-[540px] text-[14px] leading-relaxed text-ink-soft">{description}</div>
        )}
      </div>
      <div className="mt-7 flex-1">{children}</div>
      <div className="sticky bottom-0 mt-8 flex items-center justify-between gap-3 bg-gradient-to-t from-surface from-60% to-transparent pb-1 pt-5">
        {footer}
      </div>
    </>
  );
}

export function Footer({
  back,
  next,
  label = "Continue",
  disabled,
  hint,
}: {
  back?: () => void;
  next: () => void;
  label?: string;
  disabled?: boolean;
  hint?: ReactNode;
}) {
  return (
    <>
      {back ? (
        <Button variant="ghost" onClick={back}>
          <ArrowLeft />
          Back
        </Button>
      ) : (
        <span />
      )}
      <div className="flex items-center gap-4">
        {hint && <span className="text-[12.5px] text-ink-faint">{hint}</span>}
        <Button variant="primary" size="lg" disabled={disabled} onClick={next}>
          {label}
          <ArrowRight />
        </Button>
      </div>
    </>
  );
}
