import { ExternalLink, MicOff } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, Group, Row } from "@/components/ui/layout";
import { commands, type MicAccess } from "@/lib/bindings";
import { MicrophoneSelect } from "../components/MicrophoneSelect";
import { MicTest } from "../components/MicTest";
import { useLoad } from "../hooks";
import { useHub } from "../store";
import { isMac } from "@/lib/utils";
import { Footer, type StepProps, StepLayout } from "./layout";

const BLOCKED_WINDOWS: Record<Exclude<MicAccess, "allowed">, { title: string; fix: string }> = {
  blockedForDesktopApps: {
    title: "Windows is keeping desktop apps away from your microphone",
    fix: "In Settings, turn on “Let desktop apps access your microphone”, then come back here.",
  },
  blockedForUser: {
    title: "Microphone access is off",
    fix: "In Settings, turn on “Microphone access” and “Let desktop apps access your microphone”, then come back here.",
  },
  blockedForDevice: {
    title: "Microphone access is off for this PC",
    fix: "An administrator has to turn on “Microphone access” in Settings. Then come back here.",
  },
};

const BLOCKED_MAC: Record<Exclude<MicAccess, "allowed">, { title: string; fix: string }> = {
  blockedForDesktopApps: {
    title: "macOS is keeping apps away from your microphone",
    fix: "In System Settings, turn on Microphone access for Joona, then come back here.",
  },
  blockedForUser: {
    title: "Microphone access is off",
    fix: "In System Settings, turn on Microphone access for Joona, then come back here.",
  },
  blockedForDevice: {
    title: "Microphone access is off for this Mac",
    fix: "An administrator has to turn on Microphone access in System Settings. Then come back here.",
  },
};

export function MicrophoneStep({ next, back }: StepProps) {
  const device = useHub((s) => s.settings?.general.microphone ?? null);
  // Windows Settings opens in its own window, so look again when the Hub is back in front.
  const [visit, setVisit] = useState(0);
  useEffect(() => {
    const again = () => setVisit((v) => v + 1);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);
  const { data: access } = useLoad(() => commands.micAccess(), [visit]);
  const [heard, setHeard] = useState(false);
  const blockedMap = isMac ? BLOCKED_MAC : BLOCKED_WINDOWS;
  const blocked = access && access !== "allowed" ? blockedMap[access] : null;

  return (
    <StepLayout
      title="Can Joona hear you?"
      description="Say a few words. The bars should move as you talk. Nothing is recorded."
      footer={<Footer back={back} next={next} label={heard ? "Sounds good" : blocked ? "Skip for now" : "Continue"} />}
    >
      {blocked ? (
        <Card className="flex items-start gap-4 p-5">
          <span className="grid size-10 shrink-0 place-items-center rounded-full bg-danger-soft text-danger">
            <MicOff className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-semibold text-ink">{blocked.title}</p>
            <p className="mt-1 text-[13px] leading-relaxed text-ink-soft">{blocked.fix}</p>
            <Button
              variant="primary"
              className="mt-4"
              onClick={() =>
                void commands
                  .openMicPrivacy()
                  .catch((e) =>
                    toast.error(isMac ? "Couldn't open System Settings" : "Couldn't open Windows Settings", {
                      description: String(e),
                    }),
                  )
              }
            >
              {isMac ? "Open System Settings" : "Open Windows Settings"}
              <ExternalLink />
            </Button>
          </div>
        </Card>
      ) : (
        <div className="space-y-4">
          <Card className="px-6 pb-6 pt-8">{access && <MicTest device={device} large onHeard={() => setHeard(true)} />}</Card>
          <Group>
            <Row title="Microphone" description="If the bars don't move, try another one.">
              <MicrophoneSelect device={device} reload={visit} />
            </Row>
          </Group>
        </div>
      )}
    </StepLayout>
  );
}
