import { Check, Cloud, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Confirm } from "@/components/ui/confirm";
import { Group, Row } from "@/components/ui/layout";
import { SttSheet } from "../components/SttSheet";
import { useProviders } from "../providers";
import type { Settings } from "../store";
import { type SttChoice, type SttProvider, useStt } from "../stt";

/**
 * The service that hears dictations in place of the model on this PC, when
 * the user brings their own key for one.
 */
export function SpeechToText({ settings }: { settings: Settings }) {
  const overview = useStt((s) => s.overview);
  const keys = useProviders((s) => s.keys);
  const [sheet, setSheet] = useState<SttProvider | null>(null);
  const { transcription, privacy } = settings;
  // Whether the service is ready depends on these.
  useEffect(() => void useStt.getState().refresh(), [transcription, privacy.consented, keys]);
  if (!overview) return null;

  const open = (id: string) => setSheet(overview.providers.find((p) => p.id === id) ?? null);
  const { ready, problem } = overview;
  const picked = overview.providers.find((p) => p.id === transcription.provider);
  const stop = () => void useStt.getState().stop();
  return (
    <div className="mt-8">
      <Group
        title="Speech to text"
        description="Models on this PC turn your speech into text for free. A service with your own key can do better on a slow PC, or with accents and languages the models here find hard."
        action={
          ready ? (
            ready.cloud ? (
              <Badge tone="cloud">
                <Cloud />
                Cloud
              </Badge>
            ) : (
              <Badge tone="success">Your server</Badge>
            )
          ) : picked ? (
            <Badge tone="danger">Not ready</Badge>
          ) : (
            <Badge tone="success">On this PC</Badge>
          )
        }
      >
        {ready ? (
          <Row title={`${ready.name} hears your dictations`} description={inUseNote(ready)}>
            <Button variant="ghost" onClick={() => open(ready.provider)}>
              Change
            </Button>
            <Button variant="quiet" onClick={stop}>
              Stop using
            </Button>
          </Row>
        ) : (
          picked && (
            <Row
              title={`${picked.name} isn't ready`}
              description={`${problem ?? ""} Until it is, the model on this PC hears your dictations.`}
            >
              <Button variant="primary" onClick={() => open(picked.id)}>
                Fix
              </Button>
              <Button variant="quiet" onClick={stop}>
                Stop using
              </Button>
            </Row>
          )
        )}
        {overview.providers.map((p) => (
          <Service
            key={p.id}
            provider={p}
            inUse={transcription.provider === p.id}
            keySaved={p.key !== "never" && (keys?.includes(p.id) ?? false)}
            onOpen={open}
          />
        ))}
      </Group>
      <SttSheet provider={sheet} onOpenChange={(isOpen) => !isOpen && setSheet(null)} />
    </div>
  );
}

function inUseNote(ready: SttChoice): string {
  const where = ready.cloud
    ? `Your recordings go to ${ready.name} once you stop speaking, and the bar shows a cloud badge.`
    : "Your recordings go to your server on this PC.";
  const english = ready.translates ? " It turns speech in other languages straight into English." : "";
  return `${ready.model}. ${where}${english} Languages you send to Sarvam still go to Sarvam.`;
}

function Service({
  provider: p,
  inUse,
  keySaved,
  onOpen,
}: {
  provider: SttProvider;
  inUse: boolean;
  keySaved: boolean;
  onOpen: (id: string) => void;
}) {
  return (
    <Row title={p.name} description={keySaved ? `${p.summary} Key saved.` : p.summary}>
      {keySaved && (
        <Confirm
          title={`Delete your ${p.name} key?`}
          description={`Joona deletes the key from this PC and stops sending ${p.name} anything.`}
          confirm="Delete"
          onConfirm={() => void useProviders.getState().remove(p.id)}
        >
          <Button variant="ghost" size="icon-sm" aria-label={`Delete your ${p.name} key`}>
            <Trash2 />
          </Button>
        </Confirm>
      )}
      {inUse ? (
        <Badge tone="ink">
          <Check strokeWidth={3} />
          In use
        </Badge>
      ) : (
        <Button onClick={() => onOpen(p.id)}>{p.key === "required" && !keySaved ? "Add key…" : "Use…"}</Button>
      )}
    </Row>
  );
}
