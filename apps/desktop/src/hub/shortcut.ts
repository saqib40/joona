import type { Shortcut_Serialize as Shortcut } from "@/lib/bindings";

const NAMES: Record<number, string> = {
  0x05: "Mouse 4",
  0x06: "Mouse 5",
  0x08: "Backspace",
  0x09: "Tab",
  0x0d: "Enter",
  0x13: "Pause",
  0x14: "Caps Lock",
  0x1b: "Esc",
  0x20: "Space",
  0x21: "Page Up",
  0x22: "Page Down",
  0x23: "End",
  0x24: "Home",
  0x25: "Left",
  0x26: "Up",
  0x27: "Right",
  0x28: "Down",
  0x2c: "Print Screen",
  0x2d: "Insert",
  0x2e: "Delete",
  0x5d: "Menu",
  0x91: "Scroll Lock",
  0xc0: "`",
};

/** The same names as `key_name` in `hotkeys/gesture.rs`. */
export function keyName(vk: number): string {
  if ((vk >= 0x30 && vk <= 0x39) || (vk >= 0x41 && vk <= 0x5a)) return String.fromCharCode(vk);
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  if (vk >= 0x60 && vk <= 0x69) return `Num ${vk - 0x60}`;
  return NAMES[vk] ?? `Key 0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

export function shortcutKeys(s: Shortcut): string[] {
  const keys: string[] = [];
  if (s.ctrl) keys.push("Ctrl");
  if (s.win) keys.push("Win");
  if (s.alt) keys.push("Alt");
  if (s.shift) keys.push("Shift");
  if (s.key !== null) keys.push(keyName(s.key));
  return keys;
}

export const shortcutLabel = (s: Shortcut) => shortcutKeys(s).join("+");

export const sameShortcut = (a: Shortcut | null, b: Shortcut | null) =>
  !!a && !!b && a.ctrl === b.ctrl && a.win === b.win && a.alt === b.alt && a.shift === b.shift && a.key === b.key;

const RESERVED: [string, string][] = [
  ["Win+L", "locks Windows"],
  ["Alt+Tab", "switches windows"],
  ["Alt+F4", "closes the app in front"],
  ["Ctrl+Alt+Delete", "opens the security screen"],
  ["Win+D", "shows the desktop"],
  ["Win+Tab", "opens Task View"],
  ["Ctrl+C", "copies"],
  ["Ctrl+V", "pastes"],
  ["Ctrl+X", "cuts"],
  ["Ctrl+Z", "undoes"],
  ["Ctrl+A", "selects everything"],
];

/** Why Windows or most apps already need this shortcut, if they do. */
export function reservedBy(s: Shortcut): string | null {
  const label = shortcutLabel(s);
  const hit = RESERVED.find(([keys]) => keys === label);
  return hit ? `${hit[0]} ${hit[1]}` : null;
}
