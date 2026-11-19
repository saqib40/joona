//! Simulated key presses, marked so Joona's own keyboard hook ignores them.

use std::time::{Duration, Instant};

use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_C, VK_CONTROL,
    VK_RETURN, VK_SHIFT, VK_V,
};
use anyhow::bail;

use crate::hotkeys::{JOONA_INPUT_MARKER, modifiers_released};
use crate::insert::{Stroke, strokes};

/// Events per `SendInput` call when typing. Small batches keep slow apps
/// from dropping characters.
const TYPE_BATCH: usize = 64;

fn keyboard(ki: KEYBDINPUT) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    // SAFETY: a pure lookup in the current keyboard layout.
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    keyboard(KEYBDINPUT {
        wVk: vk,
        wScan: scan,
        dwFlags: if up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        },
        time: 0,
        dwExtraInfo: JOONA_INPUT_MARKER,
    })
}

fn unicode(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard(KEYBDINPUT {
        wVk: VIRTUAL_KEY(0),
        wScan: unit,
        dwFlags: flags,
        time: 0,
        dwExtraInfo: JOONA_INPUT_MARKER,
    })
}

fn send(inputs: &[INPUT]) -> anyhow::Result<()> {
    // SAFETY: the slice holds fully initialised keyboard inputs.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        bail!("Windows blocked the simulated key presses");
    }
    Ok(())
}

fn ctrl(vk: VIRTUAL_KEY) -> anyhow::Result<()> {
    send(&[
        key(VK_CONTROL, false),
        key(vk, false),
        key(vk, true),
        key(VK_CONTROL, true),
    ])
}

pub fn paste() -> anyhow::Result<()> {
    ctrl(VK_V)
}

pub fn copy() -> anyhow::Result<()> {
    ctrl(VK_C)
}

pub fn press_enter() -> anyhow::Result<()> {
    send(&[key(VK_RETURN, false), key(VK_RETURN, true)])
}

pub fn type_text(text: &str) -> anyhow::Result<()> {
    let mut inputs = Vec::with_capacity(text.len() * 2);
    for stroke in strokes(text) {
        match stroke {
            Stroke::Unit(unit) => {
                inputs.push(unicode(unit, false));
                inputs.push(unicode(unit, true));
            }
            Stroke::NewLine => inputs.extend([
                key(VK_SHIFT, false),
                key(VK_RETURN, false),
                key(VK_RETURN, true),
                key(VK_SHIFT, true),
            ]),
        }
    }
    for batch in inputs.chunks(TYPE_BATCH) {
        send(batch)?;
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

/// Waits until the user lets go of Ctrl, Win, Alt and Shift, so simulated
/// shortcuts aren't combined with them.
pub fn wait_for_modifiers(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if modifiers_released() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
