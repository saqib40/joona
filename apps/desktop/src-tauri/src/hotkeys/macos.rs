//! Low-level macOS keyboard tap on a dedicated thread using CoreGraphics CGEventTap.

use std::sync::Mutex;
use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::CFString;
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField, KeyCode,
};
use tokio::sync::mpsc::UnboundedSender;

use super::gesture::{Action, GestureConfig, GestureMachine, KeyEvent, vk};
use super::record::{Recorded, Recorder};

/// Marks input that Joona itself injects, so the hook can ignore it.
pub const JOONA_INPUT_MARKER: usize = 0x4A4F_4F4E; // "JOON"

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceFlagsState(stateID: i32) -> u64;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
}

/// Checks whether macOS Accessibility permission has been granted, optionally showing the system prompt.
pub fn is_accessibility_trusted(prompt: bool) -> bool {
    if prompt {
        let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let value = CFBoolean::true_value();
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
        unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()) }
    } else {
        unsafe { AXIsProcessTrusted() }
    }
}

/// Returns whether all modifier keys (Command, Shift, Control, Option) are physically released.
pub fn modifiers_released() -> bool {
    let flags = unsafe { CGEventSourceFlagsState(0 /* CombinedSessionState */) };
    // Command: 0x00100000, Shift: 0x00020000, Control: 0x00040000, Option: 0x00080000
    (flags & 0x001E0000) == 0
}

struct Shared {
    machine: GestureMachine,
    tx: UnboundedSender<Action>,
    epoch: Instant,
    last_flags: CGEventFlags,
    run_loop: Option<CFRunLoop>,
    recorder: Option<(Recorder, UnboundedSender<Recorded>)>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

fn with_shared<R>(f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_mut().map(f)
}

pub struct HookThread {
    join: Option<JoinHandle<()>>,
}

impl HookThread {
    pub fn start(cfg: GestureConfig, tx: UnboundedSender<Action>) -> anyhow::Result<Self> {
        let trusted = is_accessibility_trusted(true);
        if !trusted {
            tracing::warn!(
                "macOS Accessibility permission not granted yet. Global hotkeys may not register until granted."
            );
        }

        let (ready_tx, ready_rx) = std_mpsc::channel::<anyhow::Result<()>>();
        let join = std::thread::Builder::new()
            .name("joona-hotkeys-mac".into())
            .spawn(move || run_tap_thread(cfg, tx, ready_tx))?;

        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow::anyhow!("macOS hotkey thread timed out during startup"))??;

        Ok(Self { join: Some(join) })
    }

    pub fn update_config(&self, cfg: GestureConfig) {
        with_shared(|s| s.machine.set_config(cfg));
    }

    pub fn set_paused(&self, paused: bool) {
        with_shared(|s| s.machine.set_paused(paused));
    }

    pub fn session_ended(&self) {
        with_shared(|s| s.machine.session_ended());
    }

    pub fn begin_hands_free(&self) {
        with_shared(|s| s.machine.begin_hands_free());
    }

    pub fn record(&self, tx: UnboundedSender<Recorded>) {
        with_shared(|s| s.recorder = Some((Recorder::default(), tx)));
    }

    pub fn stop_recording(&self) {
        with_shared(|s| s.recorder = None);
    }
}

impl Drop for HookThread {
    fn drop(&mut self) {
        let run_loop = with_shared(|s| s.run_loop.take()).flatten();
        if let Some(rl) = run_loop {
            rl.stop();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn feed(vk_code: u16, down: bool) -> bool {
    with_shared(|s| {
        if let Some((recorder, tx)) = &mut s.recorder {
            if let Some(event) = recorder.on_key(vk_code, down) {
                let finished = !matches!(event, Recorded::Preview(_));
                let _ = tx.send(event);
                if finished {
                    s.recorder = None;
                }
            }
            return true;
        }

        let time_ms = s.epoch.elapsed().as_millis() as u64;
        let out = s.machine.on_key(KeyEvent {
            vk: vk_code,
            down,
            time_ms,
        });

        for action in out.actions {
            // Action::InjectMask is Windows-only (Start Menu suppression).
            if action != Action::InjectMask {
                let _ = s.tx.send(action);
            }
        }
        out.swallow
    })
    .unwrap_or(false)
}

fn mac_keycode_to_vk(code: u16) -> Option<u16> {
    match code {
        KeyCode::ANSI_A => Some(0x41),
        KeyCode::ANSI_B => Some(0x42),
        KeyCode::ANSI_C => Some(0x43),
        KeyCode::ANSI_D => Some(0x44),
        KeyCode::ANSI_E => Some(0x45),
        KeyCode::ANSI_F => Some(0x46),
        KeyCode::ANSI_G => Some(0x47),
        KeyCode::ANSI_H => Some(0x48),
        KeyCode::ANSI_I => Some(0x49),
        KeyCode::ANSI_J => Some(0x4A),
        KeyCode::ANSI_K => Some(0x4B),
        KeyCode::ANSI_L => Some(0x4C),
        KeyCode::ANSI_M => Some(0x4D),
        KeyCode::ANSI_N => Some(0x4E),
        KeyCode::ANSI_O => Some(0x4F),
        KeyCode::ANSI_P => Some(0x50),
        KeyCode::ANSI_Q => Some(0x51),
        KeyCode::ANSI_R => Some(0x52),
        KeyCode::ANSI_S => Some(0x53),
        KeyCode::ANSI_T => Some(0x54),
        KeyCode::ANSI_U => Some(0x55),
        KeyCode::ANSI_V => Some(0x56),
        KeyCode::ANSI_W => Some(0x57),
        KeyCode::ANSI_X => Some(0x58),
        KeyCode::ANSI_Y => Some(0x59),
        KeyCode::ANSI_Z => Some(0x5A),

        KeyCode::ANSI_0 => Some(0x30),
        KeyCode::ANSI_1 => Some(0x31),
        KeyCode::ANSI_2 => Some(0x32),
        KeyCode::ANSI_3 => Some(0x33),
        KeyCode::ANSI_4 => Some(0x34),
        KeyCode::ANSI_5 => Some(0x35),
        KeyCode::ANSI_6 => Some(0x36),
        KeyCode::ANSI_7 => Some(0x37),
        KeyCode::ANSI_8 => Some(0x38),
        KeyCode::ANSI_9 => Some(0x39),

        KeyCode::SPACE => Some(vk::SPACE),
        KeyCode::ESCAPE => Some(vk::ESCAPE),
        KeyCode::RETURN => Some(0x0D),
        KeyCode::TAB => Some(0x09),
        KeyCode::DELETE => Some(0x08),
        KeyCode::FORWARD_DELETE => Some(0x2E),

        KeyCode::LEFT_ARROW => Some(0x25),
        KeyCode::UP_ARROW => Some(0x26),
        KeyCode::RIGHT_ARROW => Some(0x27),
        KeyCode::DOWN_ARROW => Some(0x28),

        KeyCode::F1 => Some(0x70),
        KeyCode::F2 => Some(0x71),
        KeyCode::F3 => Some(0x72),
        KeyCode::F4 => Some(0x73),
        KeyCode::F5 => Some(0x74),
        KeyCode::F6 => Some(0x75),
        KeyCode::F7 => Some(0x76),
        KeyCode::F8 => Some(0x77),
        KeyCode::F9 => Some(0x78),
        KeyCode::F10 => Some(0x79),
        KeyCode::F11 => Some(0x7A),
        KeyCode::F12 => Some(0x7B),

        KeyCode::COMMAND => Some(vk::LWIN),
        KeyCode::RIGHT_COMMAND => Some(vk::RWIN),
        KeyCode::CONTROL => Some(vk::LCONTROL),
        KeyCode::RIGHT_CONTROL => Some(vk::RCONTROL),
        KeyCode::OPTION => Some(vk::LMENU),
        KeyCode::RIGHT_OPTION => Some(vk::RMENU),
        KeyCode::SHIFT => Some(vk::LSHIFT),
        KeyCode::RIGHT_SHIFT => Some(vk::RSHIFT),
        KeyCode::CAPS_LOCK => Some(vk::CAPITAL),

        _ => None,
    }
}

fn modifier_from_keycode(code: u16, flags: CGEventFlags) -> Option<(u16, bool)> {
    match code {
        KeyCode::COMMAND => Some((vk::LWIN, flags.contains(CGEventFlags::CGEventFlagCommand))),
        KeyCode::RIGHT_COMMAND => {
            Some((vk::RWIN, flags.contains(CGEventFlags::CGEventFlagCommand)))
        }
        KeyCode::CONTROL => Some((
            vk::LCONTROL,
            flags.contains(CGEventFlags::CGEventFlagControl),
        )),
        KeyCode::RIGHT_CONTROL => Some((
            vk::RCONTROL,
            flags.contains(CGEventFlags::CGEventFlagControl),
        )),
        KeyCode::OPTION => Some((vk::LMENU, flags.contains(CGEventFlags::CGEventFlagAlternate))),
        KeyCode::RIGHT_OPTION => {
            Some((vk::RMENU, flags.contains(CGEventFlags::CGEventFlagAlternate)))
        }
        KeyCode::SHIFT => Some((vk::LSHIFT, flags.contains(CGEventFlags::CGEventFlagShift))),
        KeyCode::RIGHT_SHIFT => {
            Some((vk::RSHIFT, flags.contains(CGEventFlags::CGEventFlagShift)))
        }
        KeyCode::CAPS_LOCK => Some((
            vk::CAPITAL,
            flags.contains(CGEventFlags::CGEventFlagAlphaShift),
        )),
        _ => None,
    }
}

fn run_tap_thread(
    cfg: GestureConfig,
    tx: UnboundedSender<Action>,
    ready: std_mpsc::Sender<anyhow::Result<()>>,
) {
    let run_loop = CFRunLoop::get_current();

    *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Shared {
        machine: GestureMachine::new(cfg),
        tx,
        epoch: Instant::now(),
        last_flags: CGEventFlags::empty(),
        run_loop: Some(run_loop.clone()),
        recorder: None,
    });

    let tap = match CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
        ],
        move |_proxy, etype, event| {
            if matches!(
                etype,
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
            ) {
                tracing::debug!("CGEventTap was disabled ({etype:?}), re-enabling");
                return CallbackResult::Keep;
            }

            // Ignore events injected by Joona itself.
            let marker = event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
            if marker == JOONA_INPUT_MARKER as i64 {
                return CallbackResult::Keep;
            }

            let keycode =
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

            if matches!(etype, CGEventType::FlagsChanged) {
                let flags = event.get_flags();
                let last = with_shared(|s| {
                    let prev = s.last_flags;
                    s.last_flags = flags;
                    prev
                })
                .unwrap_or_default();

                if let Some((vk_code, down)) = modifier_from_keycode(keycode, flags) {
                    if feed(vk_code, down) {
                        return CallbackResult::Drop;
                    }
                } else {
                    // Fallback comparison for modifier flags
                    let mut swallow = false;
                    for (flag_bit, vk_code) in [
                        (CGEventFlags::CGEventFlagCommand, vk::LWIN),
                        (CGEventFlags::CGEventFlagControl, vk::LCONTROL),
                        (CGEventFlags::CGEventFlagAlternate, vk::LMENU),
                        (CGEventFlags::CGEventFlagShift, vk::LSHIFT),
                        (CGEventFlags::CGEventFlagAlphaShift, vk::CAPITAL),
                    ] {
                        if flags.contains(flag_bit) != last.contains(flag_bit) {
                            if feed(vk_code, flags.contains(flag_bit)) {
                                swallow = true;
                            }
                        }
                    }
                    if swallow {
                        return CallbackResult::Drop;
                    }
                }
                return CallbackResult::Keep;
            }

            if matches!(etype, CGEventType::KeyDown | CGEventType::KeyUp) {
                let down = matches!(etype, CGEventType::KeyDown);
                if let Some(vk_code) = mac_keycode_to_vk(keycode) {
                    if feed(vk_code, down) {
                        return CallbackResult::Drop;
                    }
                }
            }

            CallbackResult::Keep
        },
    ) {
        Ok(t) => t,
        Err(_) => {
            let _ = ready.send(Err(anyhow::anyhow!(
                "Failed to install CGEventTap. Ensure Accessibility permission is granted in macOS System Settings."
            )));
            return;
        }
    };

    let mach_port = tap.mach_port();
    let source = match mach_port.create_runloop_source(0) {
        Ok(s) => s,
        Err(_) => {
            let _ = ready.send(Err(anyhow::anyhow!("Failed to create run loop source")));
            return;
        }
    };

    run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();

    let _ = ready.send(Ok(()));
    CFRunLoop::run_current();
}
