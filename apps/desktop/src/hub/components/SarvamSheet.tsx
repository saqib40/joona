import { type FormEvent, type ReactNode, useState } from "react";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { isMac } from "@/lib/utils";
import { SARVAM, SARVAM_KEYS, SARVAM_PRIVACY, useProviders, useSarvam } from "../providers";
import { saveSettings } from "../settings";
import type { Settings } from "../store";
import { TextLink as Link } from "./TextLink";

/**
 * Asks before anything goes to Sarvam the first time, and for an API key
 * when none is saved. `apply` is saved together with the consent, so nothing
 * is sent to Sarvam before the user agreed.
 */
export function SarvamSheet({
  open,
  onOpenChange,
  apply,
  replace = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  apply?: (draft: Settings) => void;
  /** Ask for a new key even though one is saved. */
  replace?: boolean;
}) {
  const sarvam = useSarvam();
  const ask = () => ({ consented: sarvam.consented, askKey: replace || !sarvam.hasKey, replace });
  // Fixed when the sheet opens, so it doesn't change while saving or closing.
  const [asked, setAsked] = useState(ask);
  const [wasOpen, setWasOpen] = useState(open);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setAsked(ask());
      setKey("");
      setError(null);
    }
  }
  const { consented, askKey } = asked;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      if (askKey) {
        const failure = await useProviders.getState().save(SARVAM, key.trim());
        if (failure) return setError(failure.message);
      }
      const saved = await saveSettings((s) => {
        if (!s.privacy.consented.includes(SARVAM)) s.privacy.consented.push(SARVAM);
        apply?.(s);
      });
      if (saved) onOpenChange(false);
    } finally {
      setBusy(false);
    }
  };

  const title = !consented ? "Turn on Sarvam" : asked.replace ? "Replace your Sarvam key" : "Add your Sarvam key";
  const action = !consented ? "Turn on Sarvam" : "Save key";
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={
        consented
          ? "Joona uses the key only to talk to Sarvam."
          : "Sarvam AI, an Indian company, understands Indian languages, accents and Hinglish better than the models that run on this PC."
      }
    >
      <form onSubmit={(e) => void submit(e)}>
        {!consented && (
          <div className="mt-5 rounded-xl bg-fill px-4 py-3.5 text-[12.5px] leading-relaxed text-ink-soft">
            <p className="font-medium text-ink">What Sarvam receives</p>
            <ul className="mt-1.5 list-disc space-y-1 pl-4">
              <li>Recordings in the languages you send to Sarvam.</li>
              <li>Text that Joona asks it to translate or write in English letters.</li>
            </ul>
            <p className="mt-2">
              Everything else stays on this PC, and the bar shows a cloud badge whenever something goes to Sarvam. Sarvam
              handles it under its <Link href={SARVAM_PRIVACY}>privacy policy</Link>. You can turn this off on the
              Providers page at any time.
            </p>
          </div>
        )}
        {askKey && (
          <div className="mt-5">
            <label htmlFor="sarvam-key" className="text-[13px] font-medium text-ink">
              API key
            </label>
            <Input
              id="sarvam-key"
              type="password"
              autoFocus
              autoComplete="off"
              spellCheck={false}
              value={key}
              placeholder="Paste your key"
              aria-invalid={error !== null}
              onChange={(e) => {
                setKey(e.target.value);
                setError(null);
              }}
              className="mt-1.5"
            />
            {error && (
              <p role="alert" className="mt-2 text-[12.5px] text-danger">
                {error}
              </p>
            )}
            <p className="mt-2 text-[12px] leading-relaxed text-ink-faint">
              Create one in your <Link href={SARVAM_KEYS}>Sarvam dashboard</Link>. Joona checks it, keeps it in {isMac ? "macOS Keychain" : "Windows Credential Manager"}, and never shows it again.
            </p>
          </div>
        )}
        <div className="mt-6 flex justify-end gap-2">
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={busy || (askKey && !key.trim())}>
            {busy && askKey ? "Checking the key…" : action}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

/**
 * Saves a change that sends something to Sarvam: straight away once Sarvam is
 * set up, or after the sheet otherwise. Render `sheet` somewhere on the page.
 */
export function useSarvamSheet(): { request: (apply: (draft: Settings) => void) => void; sheet: ReactNode } {
  const { ready } = useSarvam();
  const [pending, setPending] = useState<{ apply: (draft: Settings) => void } | null>(null);
  const request = (apply: (draft: Settings) => void) => {
    if (ready) void saveSettings(apply);
    else setPending({ apply });
  };
  const sheet = (
    <SarvamSheet open={pending !== null} onOpenChange={(open) => !open && setPending(null)} apply={pending?.apply} />
  );
  return { request, sheet };
}