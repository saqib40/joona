import { Check, CircleExclamationPoint, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Keys } from "@/components/ui/kbd";
import { Card, ProgressBar } from "@/components/ui/layout";
import { events } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { plural } from "../format";
import { downloadState, fraction, useModels } from "../models";
import { shortcutKeys } from "../shortcut";
import { useHub } from "../store";
import { Footer, type StepProps, StepLayout } from "./layout";

type Stage = "waiting" | "listening" | "writing" | "done";

export function Practice({ next, back }: StepProps) {
  const trigger = useHub((s) => s.settings?.shortcuts.trigger);
  const doubleTap = useHub((s) => s.settings?.shortcuts.doubleTapForHandsFree ?? true);
  const keys = trigger ? shortcutKeys(trigger) : ["Ctrl", "Win"];
  const [stage, setStage] = useState<Stage>("waiting");
  const [problem, setProblem] = useState<string | null>(null);
  const [words, setWords] = useState(0);
  const box = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const listening = [
      events.barStateChanged.listen(({ payload }) => {
        if (payload.state === "listening") {
          setProblem(null);
          setStage("listening");
        } else if (payload.state === "processing") {
          setStage("writing");
        } else if (payload.state === "done") {
          setStage(payload.outcome.kind === "copied" ? "waiting" : "done");
        }
      }),
      events.toastShown.listen(({ payload }) => {
        if (payload.tone === "error" || payload.tone === "warning") {
          setProblem(payload.detail ? `${payload.title}. ${payload.detail}` : payload.title);
          setStage("waiting");
        }
      }),
    ];
    return () => listening.forEach((p) => void p.then((stop) => stop()));
  }, []);

  // The text arrives by paste, so count what's in the box once it's there.
  const counted = () => {
    const text = box.current?.value.trim() ?? "";
    setWords(text ? text.split(/\s+/).length : 0);
  };

  return (
    <StepLayout
      title="Try it out"
      description={
        <>
          Hold <Keys keys={keys} />, say something like “Hi, I'm trying Joona”, and let go.
        </>
      }
      footer={<Footer back={back} next={next} label={words > 0 ? "Continue" : "Skip"} />}
    >
      <NotReady />
      <Card
        className={cn(
          "overflow-hidden transition-shadow",
          stage === "listening" && "ring-2 ring-accent",
        )}
        onClick={() => box.current?.focus()}
      >
        <div className="flex items-center justify-between gap-4 border-b border-line px-5 py-3.5">
          <Keys keys={keys} size="lg" className={cn("transition-transform", stage === "listening" && "scale-[0.97]")} />
          <Status stage={stage} />
        </div>
        <textarea
          ref={box}
          autoFocus
          rows={5}
          onInput={counted}
          placeholder="Your words appear here."
          aria-label="Practice box"
          className="block w-full resize-none bg-transparent px-5 py-4 text-[15px] leading-relaxed text-ink outline-none placeholder:text-ink-faint"
        />
      </Card>
      {problem && (
        <p role="alert" className="mt-3 flex items-start gap-2 text-[13px] leading-snug text-danger">
          <CircleExclamationPoint className="mt-0.5 size-4 shrink-0" />
          {problem}
        </p>
      )}
      {words > 0 && !problem && (
        <div className="mt-4 rounded-xl bg-success-soft px-5 py-4 animate-fade-in">
          <p className="text-[14px] font-semibold text-success">That's it. You dictated {plural(words, "word")}.</p>
          <p className="mt-1 text-[13px] leading-relaxed text-ink-soft">
            {doubleTap ? (
              <>
                For longer thoughts, double-tap <Keys keys={keys} /> to talk hands-free, and tap again to finish.
              </>
            ) : (
              "Press Esc while talking to cancel."
            )}
          </p>
        </div>
      )}
    </StepLayout>
  );
}

function Status({ stage }: { stage: Stage }) {
  const content = {
    waiting: <span className="text-ink-faint">Hold to talk</span>,
    listening: (
      <span className="flex items-center gap-2 font-medium text-accent-ink">
        <span className="relative flex size-2">
          <span className="absolute inset-0 animate-ping rounded-full bg-accent opacity-70" />
          <span className="relative size-2 rounded-full bg-accent" />
        </span>
        Listening… let go when you're done
      </span>
    ),
    writing: (
      <span className="flex items-center gap-2 text-ink-soft">
        <LoaderCircle className="size-3.5 animate-spin" />
        Writing…
      </span>
    ),
    done: (
      <span className="flex items-center gap-1.5 font-medium text-success">
        <Check className="size-4" strokeWidth={2.5} />
        Typed
      </span>
    ),
  }[stage];
  return (
    <span aria-live="polite" className="text-[13px]">
      {content}
    </span>
  );
}

/** Practising needs a speech model. */
function NotReady() {
  const overview = useModels((s) => s.overview);
  const downloads = useModels((s) => s.downloads);
  if (!overview || overview.current) return null;
  const running = Object.values(downloads).find((p) => downloadState(p).kind === "running");
  return (
    <Card className="mb-4 px-5 py-4">
      {running ? (
        <>
          <p className="text-[13.5px] font-medium text-ink">Your speech model is still downloading</p>
          <p className="mb-3 mt-0.5 text-[12.5px] text-ink-soft">You can try Joona as soon as it's done.</p>
          <ProgressBar value={fraction(running)} />
        </>
      ) : (
        <div className="flex items-center justify-between gap-4">
          <div>
            <p className="text-[13.5px] font-medium text-ink">No speech model yet</p>
            <p className="mt-0.5 text-[12.5px] text-ink-soft">Download one in the previous step to try Joona here.</p>
          </div>
          <Button size="sm" onClick={() => useModels.getState().refresh()}>
            Check again
          </Button>
        </div>
      )}
    </Card>
  );
}
