import { RefreshCw } from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { commands } from "@/lib/bindings";
import { type AiProvider, aiConsent, looksLikeAddress, sendsAway, useAi } from "../ai";
import { asFailure, consentedTo, useProviders } from "../providers";
import { useHub } from "../store";
import { TextLink } from "./TextLink";

interface Asked {
  provider: AiProvider;
  consented: boolean;
  keySaved: boolean;
  /** Counts openings, to look up models once each time. */
  opened: number;
}

/**
 * Sets up an AI provider for Smart polish: the server's address, the API key
 * and the model. The first time text would leave this PC it also asks for
 * permission, which is saved with the choice so nothing is sent before.
 */
export function AiSheet({
  provider,
  onOpenChange,
}: {
  provider: AiProvider | null;
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
  const [found, setFound] = useState<string[]>([]);
  const [listing, setListing] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (provider) {
      const current = settings?.polish.provider === provider.id;
      setAsked({
        provider,
        consented: consentedTo(settings, aiConsent(provider.id)),
        keySaved: keys?.includes(provider.id) ?? false,
        opened: (asked?.opened ?? 0) + 1,
      });
      setEndpoint((current && settings?.polish.endpoint) || "");
      setModel((current && settings?.polish.model) || provider.models[0] || "");
      setKey("");
      setReplacing(false);
      setFound([]);
      setListError(null);
      setError(null);
    }
  }

  const p = asked?.provider;
  const canKey = p !== undefined && p.key !== "never";
  const showKeyInput = canKey && (!asked?.keySaved || replacing);
  const address = p?.endpoint ? endpoint.trim() || null : null;
  const typedKey = showKeyInput ? key.trim() || null : null;

  const list = async () => {
    if (!p) return;
    setListing(true);
    setListError(null);
    try {
      const models = await commands.aiModels(p.id, typedKey, address);
      setFound(models);
      if (models.length === 0) setListError(`${p.name} didn't list any models.`);
      else setModel((m) => m.trim() || models[0]);
    } catch (e) {
      setListError(asFailure(e).message);
    } finally {
      setListing(false);
    }
  };

  // Servers people run have no suggestions, so ask them once per opening,
  // with what the sheet opened with.
  const opened = asked?.opened ?? 0;
  useEffect(() => {
    if (!open || !p || p.models.length > 0) return;
    const reachable = (p.key !== "required" || asked?.keySaved) && (!p.endpoint || p.defaultEndpoint || address);
    if (reachable) void list();
  }, [opened]);

  if (!asked || !p) return null;
  const cloud = sendsAway(p, address);
  const askConsent = cloud && !asked.consented;
  const missing =
    (p.endpoint && !p.defaultEndpoint && !address) ||
    (p.key === "required" && !asked.keySaved && !key.trim()) ||
    (!model.trim() && p.models.length === 0);
  const options = [...new Set([...p.models, ...found])];

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy || missing) return;
    if (address && !looksLikeAddress(address)) return setError("That address doesn't look right. Try http://localhost:11434.");
    setBusy(true);
    setError(null);
    try {
      if (typedKey) {
        const failure = await useProviders.getState().save(p.id, typedKey, address);
        if (failure) return setError(failure.message);
      }
      const picked = model.trim();
      const used = await useAi.getState().use(p.id, (s) => {
        s.polish.model = picked && picked !== p.models[0] ? picked : null;
        s.polish.endpoint = address;
        const consent = aiConsent(p.id);
        if (cloud && !s.privacy.consented.includes(consent)) s.privacy.consented.push(consent);
      });
      if (!used) return;
      onOpenChange(false);
      const failure = await useAi.getState().test();
      if (failure) toast.error(`${p.name} didn't work`, { description: failure.message });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={`Use ${p.name}`} description={p.summary}>
      <form onSubmit={(e) => void submit(e)}>
        {p.endpoint && (
          <Field
            id="ai-endpoint"
            label="Server address"
            hint={
              address === null && !p.defaultEndpoint
                ? "The address of a server with an OpenAI-compatible API."
                : cloud
                  ? "This server isn't on this PC, so your text travels to it."
                  : "This server is on this PC, so your text stays here."
            }
          >
            <Input
              id="ai-endpoint"
              autoFocus
              autoComplete="off"
              spellCheck={false}
              value={endpoint}
              placeholder={p.defaultEndpoint ?? "http://localhost:8080/v1"}
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
              id="ai-key"
              label={p.key === "optional" ? "API key (optional)" : "API key"}
              hint={
                <>
                  {p.keysUrl && (
                    <>
                      Create one on <TextLink href={p.keysUrl}>{p.name}'s website</TextLink>.{" "}
                    </>
                  )}
                  Joona checks it, keeps it in Windows Credential Manager, and never shows it again.
                </>
              }
            >
              <Input
                id="ai-key"
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
              <p className="text-[12.5px] text-ink-soft">Your {p.name} key is saved in Windows Credential Manager.</p>
              <Button size="sm" variant="ghost" onClick={() => setReplacing(true)}>
                Replace
              </Button>
            </div>
          ))}
        <Field
          id="ai-model"
          label="Model"
          aside={
            <Button size="sm" variant="ghost" className="-my-1" disabled={listing} onClick={() => void list()}>
              <RefreshCw className={listing ? "animate-spin" : undefined} />
              {listing ? "Looking…" : "List models"}
            </Button>
          }
          hint={
            listError ??
            (p.models.length > 0
              ? `Joona suggests ${p.models[0]}: quick, and good at dictation.`
              : "Pick one your server has, or type its name.")
          }
        >
          <Input
            id="ai-model"
            list="ai-model-options"
            autoComplete="off"
            spellCheck={false}
            value={model}
            placeholder={p.models[0] ?? "Model name"}
            onChange={(e) => setModel(e.target.value)}
          />
          <datalist id="ai-model-options">
            {options.map((m) => (
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

function Consent({ provider }: { provider: AiProvider }) {
  return (
    <div className="mt-5 rounded-xl bg-fill px-4 py-3.5 text-[12.5px] leading-relaxed text-ink-soft">
      <p className="font-medium text-ink">What {provider.name} receives</p>
      <ul className="mt-1.5 list-disc space-y-1 pl-4">
        <li>The text of each dictation while Smart polish is on, with your dictionary words.</li>
        <li>Text that Joona asks it to translate or write in English letters.</li>
      </ul>
      <p className="mt-2">
        Never your recordings. The bar shows a cloud badge whenever text goes to {provider.name}
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
