import { ProgressBar } from "@/components/ui/layout";
import { cn } from "@/lib/utils";
import { formatBytes } from "../format";
import { fraction, type Progress } from "../models";

/** A download's bar and what it's doing, under a model's card. */
export function DownloadProgress({ progress }: { progress: Progress }) {
  const share = fraction(progress);
  const percent = share === null ? null : Math.floor(share * 100);
  const text = (() => {
    switch (progress.stage) {
      case "downloading":
        return `Downloading${percent === null ? "" : ` ${percent}%`} · ${formatBytes(progress.done)} of ${formatBytes(progress.total)}${progress.bytesPerSec > 0 ? ` · ${formatBytes(progress.bytesPerSec)}/s` : ""}`;
      case "verifying":
        return "Checking the download…";
      case "converting":
        return `Converting for whisper.cpp${percent === null ? "" : ` ${percent}%`}`;
      case "unpacking":
        return "Unpacking…";
      case "cancelled":
        return `Paused at ${formatBytes(progress.done)} of ${formatBytes(progress.total)}. Resuming picks up from there.`;
      case "failed":
        return progress.error ?? "The download failed.";
      default:
        return "";
    }
  })();
  const failed = progress.stage === "failed";
  return (
    <div className="mt-3.5 space-y-1.5">
      {!failed && <ProgressBar value={share} />}
      <p className={cn("text-[12px] text-ink-soft tabular", failed && "text-danger")}>{text}</p>
    </div>
  );
}
