import { describe, expect, it } from "vitest";
import type { Shortcut_Serialize as Shortcut } from "@/lib/bindings";
import { isMac } from "@/lib/utils";
import { keyName, reservedBy, sameShortcut, shortcutKeys, shortcutLabel } from "./shortcut";

const keys = (s: Partial<Shortcut>): Shortcut => ({ ctrl: false, win: false, alt: false, shift: false, key: null, ...s });

describe("keyName", () => {
  it("names keys like the Rust side", () => {
    expect(keyName(0x41)).toBe("A");
    expect(keyName(0x39)).toBe("9");
    expect(keyName(0x70)).toBe("F1");
    expect(keyName(0x87)).toBe("F24");
    expect(keyName(0x60)).toBe("Num 0");
    expect(keyName(0x20)).toBe("Space");
    expect(keyName(0x05)).toBe("Mouse 4");
    expect(keyName(0x14)).toBe("Caps Lock");
    expect(keyName(0xba)).toBe("Key 0xBA");
  });
});

describe("shortcuts", () => {
  const ctrlWin = keys({ ctrl: true, win: true });
  const altShiftZ = keys({ alt: true, shift: true, key: 0x5a });

  it("lists modifiers in a fixed order", () => {
    expect(shortcutKeys(keys({ shift: true, win: true, ctrl: true, alt: true, key: 0x20 }))).toEqual([
      "Ctrl",
      isMac ? "Cmd" : "Win",
      isMac ? "Option" : "Alt",
      "Shift",
      "Space",
    ]);
    expect(shortcutLabel(ctrlWin)).toBe(isMac ? "Ctrl+Cmd" : "Ctrl+Win");
    expect(shortcutLabel(altShiftZ)).toBe(isMac ? "Option+Shift+Z" : "Alt+Shift+Z");
  });

  it("compares shortcuts", () => {
    expect(sameShortcut(ctrlWin, keys({ win: true, ctrl: true }))).toBe(true);
    expect(sameShortcut(ctrlWin, altShiftZ)).toBe(false);
    expect(sameShortcut(ctrlWin, null)).toBe(false);
    expect(sameShortcut(null, null)).toBe(false);
  });

  it("knows the shortcuts the operating system and most apps need", () => {
    if (isMac) {
      expect(reservedBy(keys({ win: true, key: 0x51 }))).toBe("Cmd+Q quits the app");
      expect(reservedBy(keys({ win: true, key: 0x57 }))).toBe("Cmd+W closes the window");
    } else {
      expect(reservedBy(keys({ win: true, key: 0x4c }))).toBe("Win+L locks Windows");
      expect(reservedBy(keys({ ctrl: true, key: 0x56 }))).toBe("Ctrl+V pastes");
    }
    expect(reservedBy(ctrlWin)).toBeNull();
    expect(reservedBy(altShiftZ)).toBeNull();
  });
});
