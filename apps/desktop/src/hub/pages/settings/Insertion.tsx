import { Plus, X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Group, Row } from "@/components/ui/layout";
import { Segmented } from "@/components/ui/segmented";
import { Switch } from "@/components/ui/switch";
import { commands, type Method } from "@/lib/bindings";
import { useLoad } from "../../hooks";
import { saveSettings } from "../../settings";
import type { Settings } from "../../store";

const METHODS: { value: Method; label: string }[] = [
  { value: "paste", label: "Paste" },
  { value: "type", label: "Type" },
];

export function Insertion({ settings }: { settings: Settings }) {
  const { insertion } = settings;
  return (
    <>
      <Group title="How text goes in">
        <Row
          title="Method"
          description="Pasting is instant. Typing works in apps that block pasting, but takes a moment for long text."
        >
          <Segmented
            label="Method"
            value={insertion.method}
            options={METHODS}
            onChange={(method) => void saveSettings((s) => void (s.insertion.method = method))}
          />
        </Row>
        <Row title="Keep my clipboard" description="Puts back whatever you had copied after pasting.">
          <Switch
            aria-label="Keep my clipboard"
            checked={insertion.restoreClipboard}
            onCheckedChange={(on) => void saveSettings((s) => void (s.insertion.restoreClipboard = on))}
          />
        </Row>
        <Row title="Space after each dictation" description="So the next sentence doesn't run into this one.">
          <Switch
            aria-label="Space after each dictation"
            checked={insertion.trailingSpace}
            onCheckedChange={(on) => void saveSettings((s) => void (s.insertion.trailingSpace = on))}
          />
        </Row>
      </Group>
      <Overrides settings={settings} />
    </>
  );
}

function Overrides({ settings }: { settings: Settings }) {
  const overrides = settings.insertion.overrides;
  const [exe, setExe] = useState("");
  const [method, setMethod] = useState<Method>("type");
  const { data: insights } = useLoad(() => commands.insights(), []);
  const recent = (insights?.apps ?? []).map((a) => a.app).filter((a) => !overrides.some((o) => same(o.exe, a)));

  const name = normalise(exe);
  const duplicate = !!name && overrides.some((o) => same(o.exe, name));

  const add = (e: FormEvent) => {
    e.preventDefault();
    if (!name || duplicate) return;
    void saveSettings((s) => void s.insertion.overrides.push({ exe: name, method }));
    setExe("");
  };

  return (
    <Group
      title="App exceptions"
      description="Apps where Joona should insert text differently. Joona already types into terminals that can't paste."
    >
      {overrides.map((override, i) => (
        <Row key={override.exe} title={override.exe}>
          <Segmented
            label={`Method for ${override.exe}`}
            value={override.method ?? settings.insertion.method}
            options={METHODS}
            onChange={(m) => void saveSettings((s) => void (s.insertion.overrides[i].method = m))}
          />
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={`Remove ${override.exe}`}
            onClick={() => void saveSettings((s) => void s.insertion.overrides.splice(i, 1))}
          >
            <X />
          </Button>
        </Row>
      ))}
      <form onSubmit={add} className="flex items-center gap-2 px-4 py-3">
        <Input
          value={exe}
          onChange={(e) => setExe(e.target.value)}
          placeholder="App, for example slack.exe"
          aria-label="App to add"
          list="joona-recent-apps"
          className="flex-1"
        />
        <datalist id="joona-recent-apps">
          {recent.map((app) => (
            <option key={app} value={app} />
          ))}
        </datalist>
        <Segmented label="Method for the new app" value={method} options={METHODS} onChange={setMethod} />
        <Button type="submit" disabled={!name || duplicate}>
          <Plus />
          Add
        </Button>
      </form>
      {duplicate && <p className="px-4 pb-3 text-[12px] text-ink-soft">{name} is already on the list.</p>}
    </Group>
  );
}

function normalise(exe: string): string {
  const name = exe.trim().split(/[\\/]/).pop() ?? "";
  if (!name) return "";
  return name.toLowerCase().endsWith(".exe") ? name : `${name}.exe`;
}

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
