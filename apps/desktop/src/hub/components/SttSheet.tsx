import { type FormEvent, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { commands } from "@/lib/bindings";
import { isMac } from "@/lib/utils";
import { looksLikeAddress } from "../ai";
import { listNames } from "../format";
import { asFailure, consentedTo, useProviders } from "../providers";
import { useHub } from "../store";
import { type SttProvider, sendsAway, sttConsent, useStt } from "../stt";
import { TextLink } from "./TextLink";

interface Asked {
  provider: SttProvider;
  consented: boolean;
  keySaved: boolean;
}

/**
 * Sets up a speech-to-text service to hear dictations: the server's address,
 * the API key and the model. The first time recordings would leave this PC
 * it also asks for permission, which is saved with the choice so nothing is
 * sent before.
 */
export function SttSheet({
  provider,
  onOpenChange,
}: {
  provider: SttProvider | null;
  onOpenChange: (open: boolean) => void;
}) {
  const open = provider !== null;
  const settings = useHub((s) => s.settings);
  const keys = useProviders((s) => s.keys);
  // Fixed when the sheet opens, so it doesn't change while saving or closing.
  const [asked, setAsked] = useState<Asked | null>(null);
  const [wasOpen, setWasOpen] = useState(false);
  const [endpoint, setEndpoint] = useState("");
  const [key, setKey] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [model, setModel] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (provider) {
      const current = settings?.transcription.provider === provider.id;
      setAsked({
        provider,
        consented: consentedTo(settings, sttConsent(provider.id)),
        keySaved: keys?.includes(provider.id) ?? false,
      });
      setEndpoint((current && settings?.transcription.endpoint) || "");
      setModel((current && settings?.transcription.model) || provider.models[0] || "");
      setKey("");
      setReplacing(false);
      setError(null);
    }
  }

  if (!asked) return null;
  const p = asked.provider;
  const canKey = p.key !== "never";
  const showKeyInput = canKey && (!asked.keySaved || replacing);
  const address = p.endpoint ? endpoint.trim() || null : null;
  const typedKey = showKeyInput ? key.trim() || null : null;
  const cloud = sendsAway(p, address);
  const askConsent = cloud && !asked.consented;
  const missing =
    (p.endpoint && !address) || (p.key === "required" && !asked.keySaved && !key.trim()) || !model.trim();

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy || missing) return;
    if (address && !looksLikeAddress(address)) return setError("That address doesn't look right. Try http://localhost:8000.");
    setBusy(true);
    setError(null);
    try {
      if (typedKey) {
        const failure = await useProviders.getState().save(p.id, typedKey, address);
        if (failure) return setError(failure.message);
      } else if (address) {
        // A server without a key: make sure it answers at that address.
        const failure = await commands.providerTest(p.id, null, address).then(
          () => null,
          (e: unknown) => asFailure(e),
        );
        if (failure) return setError(failure.message);
      }
      const picked = model.trim();
      const used = await useStt.getState().use(p.id, (s) => {
        s.transcription.model = picked !== p.models[0] ? picked : null;
        s.transcription.endpoint = address;
        const consent = sttConsent(p.id);
        if (cloud && !s.privacy.consented.includes(consent)) s.privacy.consented.push(consent);
      });
      if (!used) return;
      onOpenChange(false);
      toast.success(`${p.name} hears your dictations now`, {
        description: cloud ? "The bar shows a cloud badge while it listens." : undefined,
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={`Use ${p.name}`} description={p.summary}>
      <form onSubmit={(e) => void submit(e)}>
        {p.endpoint && (
          <Field
            id="stt-endpoint"
            label="Server address"
            hint={
              address === null
                ? "The address of a Whisper server with an OpenAI-compatible API."
                : cloud
                  ? "This server isn't on this PC, so your recordings travel to it."
                  : "This server is on this PC, so your recordings stay here."
            }
          >
            <Input
              id="stt-endpoint"
              autoFocus
              autoComplete="off"
              spellCheck={false}
              value={endpoint}
              placeholder="http://localhost:8000/v1"
              onChange={(e) => {
                setEndpoint(e.target.value);
                setError(null);
              }}
            />
          </Field>
        )}
        {canKey &&
          (showKeyInput ? (
            <Field
              id="stt-key"
              label={p.key === "optional" ? "API key (optional)" : "API key"}
              hint={
                <>
                  {p.keysUrl && (
                    <>
                      Create one on <TextLink href={p.keysUrl}>{p.name}'s website</TextLink>.{" "}
                    </>
                  )}
                  Joona checks it, keeps it in {isMac ? "macOS Keychain" : "Windows Credential Manager"}, and never shows it again.
                </>
              }
            >
              <Input
                id="stt-key"
                type="password"
                autoFocus={!p.endpoint}
                autoComplete="off"
                spellCheck={false}
                value={key}
                placeholder="Paste your key"
                aria-invalid={error !== null}
                onChange={(e) => {
                  setKey(e.target.value);
                  setError(null);
                }}
              />
            </Field>
          ) : (
            <div className="mt-5 flex items-center justify-between gap-4 rounded-lg bg-fill px-3.5 py-2.5">
              <p className="text-[12.5px] text-ink-soft">
                Your {p.name} key is saved in {isMac ? "macOS Keychain" : "Windows Credential Manager"}.
              </p>
              <Button size="sm" variant="ghost" onClick={() => setReplacing(true)}>
                Replace
              </Button>
            </div>
          ))}
        <Field id="stt-model" label="Model" hint={modelHint(p)}>
          <Input
            id="stt-model"
            list="stt-model-options"
            autoComplete="off"
            spellCheck={false}
            value={model}
            placeholder={p.models[0] ?? "whisper-1"}
            onChange={(e) => setModel(e.target.value)}
          />
          <datalist id="stt-model-options">
            {p.models.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </Field>
        {askConsent && <Consent provider={p} />}
        {error && (
          <p role="alert" className="mt-4 text-[12.5px] text-danger">
            {error}
          </p>
        )}
        <div className="mt-6 flex justify-end gap-2">
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={busy || missing}>
            {busy ? "Checking…" : `Use ${p.name}`}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

function modelHint(provider: SttProvider): string {
  if (provider.models.length === 0) return "The name your server gives its Whisper model.";
  const suggested = `Joona suggests ${provider.models[0]}.`;
  if (provider.translates.length === 0) return suggested;
  return `${suggested} ${listNames(provider.translates)} can also turn speech in other languages straight into English.`;
}

function Consent({ provider }: { provider: SttProvider }) {
  return (
    <div className="mt-5 rounded-xl bg-fill px-4 py-3.5 text-[12.5px] leading-relaxed text-ink-soft">
      <p className="font-medium text-ink">What {provider.name} receives</p>
      <ul className="mt-1.5 list-disc space-y-1 pl-4">
        <li>The recording of each dictation, once you stop speaking.</li>
        {provider.terms && <li>The words in your dictionary, so it spells them your way.</li>}
      </ul>
      <p className="mt-2">
        Languages you send to Sarvam still go to Sarvam. The bar shows a cloud badge whenever a recording goes to{" "}
        {provider.name}
        {provider.privacyUrl ? (
          <>
            , which handles it under its <TextLink href={provider.privacyUrl}>privacy policy</TextLink>.
          </>
        ) : (
          "."
        )}{" "}
        {provider.note}
      </p>
    </div>
  );
}
