import { Monitor, Moon, Sun } from "lucide-react";
import { Keys } from "@/components/ui/kbd";
import { Group, Row } from "@/components/ui/layout";
import { Segmented } from "@/components/ui/segmented";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { BarVisibility, Theme } from "@/lib/bindings";
import { MicrophoneSelect, useMicrophones } from "../../components/MicrophoneSelect";
import { MicTest } from "../../components/MicTest";
import { ShortcutField } from "../../components/ShortcutField";
import { saveSettings } from "../../settings";
import { shortcutKeys } from "../../shortcut";
import type { Settings } from "../../store";

const SPACE = 0x20;

export function General({ settings }: { settings: Settings }) {
  const { shortcuts, general } = settings;
  const usesSpace = shortcuts.trigger.key === SPACE;
  return (
    <>
      <Group title="Shortcuts">
        <Row title="Dictate" description="Hold to talk, let go to type. Works in every app.">
          <ShortcutField
            label="the dictation shortcut"
            value={shortcuts.trigger}
            taken={[{ shortcut: shortcuts.pasteLast, name: "paste last dictation" }]}
            onChange={(shortcut) => shortcut && void saveSettings((s) => void (s.shortcuts.trigger = shortcut))}
          />
        </Row>
        <Row
          title="Double-tap for hands-free"
          description="Tap the shortcut twice to keep listening without holding it. Tap once more to finish."
        >
          <Switch
            aria-label="Double-tap for hands-free"
            checked={shortcuts.doubleTapForHandsFree}
            onCheckedChange={(on) => void saveSettings((s) => void (s.shortcuts.doubleTapForHandsFree = on))}
          />
        </Row>
        <Row
          title="Space for hands-free"
          description={
            usesSpace ? (
              "Not available while your shortcut uses Space."
            ) : (
              <span className="inline-flex flex-wrap items-center gap-1">
                Press <Keys keys={[...shortcutKeys(shortcuts.trigger), "Space"]} /> to start hands-free.
              </span>
            )
          }
        >
          <Switch
            aria-label="Space for hands-free"
            checked={shortcuts.spaceForHandsFree && !usesSpace}
            disabled={usesSpace}
            onCheckedChange={(on) => void saveSettings((s) => void (s.shortcuts.spaceForHandsFree = on))}
          />
        </Row>
        <Row title="Paste last dictation" description="Types your last dictation again, for when it landed in the wrong place.">
          <ShortcutField
            optional
            label="the paste last dictation shortcut"
            value={shortcuts.pasteLast}
            taken={[{ shortcut: shortcuts.trigger, name: "dictation" }]}
            onChange={(shortcut) => void saveSettings((s) => void (s.shortcuts.pasteLast = shortcut))}
          />
        </Row>
      </Group>

      <Microphone device={general.microphone} />

      <Group title="Joona">
        <Row title="Start with Windows" description="Joona waits quietly in the tray until you need it.">
          <Switch
            aria-label="Start with Windows"
            checked={general.launchAtLogin}
            onCheckedChange={(on) => void saveSettings((s) => void (s.general.launchAtLogin = on))}
          />
        </Row>
        <Row title="Sounds" description="A soft chime when listening starts and stops.">
          <Switch
            aria-label="Sounds"
            checked={general.sounds}
            onCheckedChange={(on) => void saveSettings((s) => void (s.general.sounds = on))}
          />
        </Row>
        <Row title="Joona Bar" description="The small bar at the bottom of the screen.">
          <Select<BarVisibility>
            label="Joona Bar"
            value={general.bar}
            options={[
              { value: "always", label: "Always show" },
              { value: "dictating", label: "Only while dictating" },
              { value: "hidden", label: "Never show" },
            ]}
            onChange={(bar) => void saveSettings((s) => void (s.general.bar = bar))}
          />
        </Row>
        <Row title="Appearance">
          <Segmented<Theme>
            label="Appearance"
            value={general.theme}
            options={[
              { value: "system", label: "System", icon: <Monitor /> },
              { value: "light", label: "Light", icon: <Sun /> },
              { value: "dark", label: "Dark", icon: <Moon /> },
            ]}
            onChange={(theme) => void saveSettings((s) => void (s.general.theme = theme))}
          />
        </Row>
      </Group>
    </>
  );
}

function Microphone({ device }: { device: string | null }) {
  const { missing } = useMicrophones(device);
  return (
    <Group title="Microphone">
      <Row
        title="Microphone"
        description={
          missing
            ? "This microphone isn't connected. Joona uses the Windows default until it's back."
            : "Joona only listens while you dictate."
        }
      >
        <MicrophoneSelect device={device} />
      </Row>
      <Row title="Test your microphone" description="Speak and watch the level. Nothing is recorded.">
        <MicTest device={device} />
      </Row>
    </Group>
  );
}
