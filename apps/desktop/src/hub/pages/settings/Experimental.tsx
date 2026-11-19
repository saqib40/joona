import { Keys } from "@/components/ui/kbd";
import { Group, Row } from "@/components/ui/layout";
import { Switch } from "@/components/ui/switch";
import { saveSettings } from "../../settings";
import { shortcutKeys } from "../../shortcut";
import type { Settings } from "../../store";

export function Experimental({ settings }: { settings: Settings }) {
  const { experimental } = settings;
  return (
    <Group title="Experimental" description="Early features. They may change, and may not work in every app.">
      <CommandModeRow settings={settings} />
      <Row title="“Press enter”" description="End with “press enter” to send the message as well.">
        <Switch
          aria-label="Press enter"
          checked={experimental.pressEnter}
          onCheckedChange={(on) => void saveSettings((s) => void (s.experimental.pressEnter = on))}
        />
      </Row>
    </Group>
  );
}

/** Turns Command Mode on or off. It needs Alt, which a shortcut may already use. */
export function CommandModeRow({ settings }: { settings: Settings }) {
  const { experimental, shortcuts } = settings;
  const usesAlt = shortcuts.trigger.alt;
  return (
    <Row
      title="Command Mode"
      description={
        usesAlt ? (
          "Not available while your shortcut uses Alt."
        ) : (
          <span className="inline-flex flex-wrap items-center gap-1">
            Select text, hold <Keys keys={[...shortcutKeys(shortcuts.trigger), "Alt"]} /> and say what to change,
            like “make this friendlier”. With nothing selected, it writes what you ask for. Uses your AI model.
          </span>
        )
      }
    >
      <Switch
        aria-label="Command Mode"
        checked={experimental.commandMode && !usesAlt}
        disabled={usesAlt}
        onCheckedChange={(on) => void saveSettings((s) => void (s.experimental.commandMode = on))}
      />
    </Row>
  );
}
