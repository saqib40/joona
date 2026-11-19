//! The dictation gesture state machine.
//!
//! This is pure logic: it receives key presses and releases with a millisecond
//! timestamp and decides two things for every event. First, whether the key
//! should be swallowed (hidden from the focused app). Second, which actions the
//! session should take. The Windows hook calls it synchronously, so every call
//! must stay cheap.
//!
//! Gestures, with the default trigger Ctrl+Win:
//! - Hold the trigger to dictate. Releasing before [`MIN_HOLD_MS`] is a tap and
//!   pastes nothing.
//! - Double-tap the trigger, or press trigger+Space, for hands-free. Press the
//!   trigger again to stop.
//! - Esc cancels while dictating.
//! - Any other key within [`ACCIDENTAL_WINDOW_MS`] of starting cancels and is
//!   passed through, so shortcuts such as Ctrl+Win+D keep working.
//! - Trigger+Alt is Command Mode, trigger+Shift is the alternate output language.

use serde::{Deserialize, Serialize};
use specta::Type;

pub const MIN_HOLD_MS: u64 = 500;
pub const DOUBLE_TAP_MS: u64 = 400;
pub const ACCIDENTAL_WINDOW_MS: u64 = 1_000;

pub mod vk {
    pub const XBUTTON1: u16 = 0x05;
    pub const XBUTTON2: u16 = 0x06;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12;
    pub const CAPITAL: u16 = 0x14;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
    /// Unassigned virtual key, injected so a lone Win release doesn't open Start.
    pub const MASK: u16 = 0xE8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Win,
    Alt,
    Shift,
}

impl Modifier {
    pub fn of(vk: u16) -> Option<Modifier> {
        match vk {
            vk::CONTROL | vk::LCONTROL | vk::RCONTROL => Some(Modifier::Ctrl),
            vk::LWIN | vk::RWIN => Some(Modifier::Win),
            vk::MENU | vk::LMENU | vk::RMENU => Some(Modifier::Alt),
            vk::SHIFT | vk::LSHIFT | vk::RSHIFT => Some(Modifier::Shift),
            _ => None,
        }
    }
}

/// A key combination: any set of modifiers plus at most one other key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub ctrl: bool,
    pub win: bool,
    pub alt: bool,
    pub shift: bool,
    /// Virtual-key code of the non-modifier key, if any.
    pub key: Option<u16>,
}

impl Shortcut {
    pub const CTRL_WIN: Shortcut = Shortcut {
        ctrl: true,
        win: true,
        alt: false,
        shift: false,
        key: None,
    };

    pub const ALT_SHIFT_Z: Shortcut = Shortcut {
        ctrl: false,
        win: false,
        alt: true,
        shift: true,
        key: Some(0x5A),
    };

    fn requires(&self, m: Modifier) -> bool {
        match m {
            Modifier::Ctrl => self.ctrl,
            Modifier::Win => self.win,
            Modifier::Alt => self.alt,
            Modifier::Shift => self.shift,
        }
    }

    pub fn is_valid_trigger(&self) -> bool {
        let mods = [self.ctrl, self.win, self.alt, self.shift]
            .iter()
            .filter(|m| **m)
            .count();
        // A lone letter or digit would make that key unusable for typing.
        match self.key {
            None => mods >= 1,
            Some(k) => mods >= 1 || !is_typing_key(k),
        }
    }

    /// How the shortcut is written in menus and hints, for example `Ctrl+Win` on Windows or `Ctrl+Cmd` on macOS.
    pub fn label(&self) -> String {
        let win_label = if cfg!(target_os = "macos") { "Cmd" } else { "Win" };
        let alt_label = if cfg!(target_os = "macos") { "Option" } else { "Alt" };
        let mut parts: Vec<String> = [
            (self.ctrl, "Ctrl"),
            (self.win, win_label),
            (self.alt, alt_label),
            (self.shift, "Shift"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| (*name).to_owned())
        .collect();
        if let Some(k) = self.key {
            parts.push(key_name(k));
        }
        parts.join("+")
    }
}

fn key_name(vk: u16) -> String {
    let name = match vk {
        0x30..=0x39 | 0x41..=0x5A => return char::from(vk as u8).to_string(),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        0x60..=0x69 => return format!("Num {}", vk - 0x60),
        vk::XBUTTON1 => "Mouse 4",
        vk::XBUTTON2 => "Mouse 5",
        vk::SPACE => "Space",
        vk::CAPITAL => "Caps Lock",
        vk::ESCAPE => "Esc",
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5D => "Menu",
        0x91 => "Scroll Lock",
        0xC0 => "`",
        _ => return format!("Key {vk:#04X}"),
    };
    name.to_owned()
}

fn is_typing_key(vk: u16) -> bool {
    matches!(vk, 0x08 | 0x09 | 0x0D | 0x20 | 0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GestureConfig {
    pub trigger: Shortcut,
    /// Trigger+Space starts hands-free.
    pub space_for_hands_free: bool,
    pub double_tap_for_hands_free: bool,
    /// Trigger+Alt is Command Mode.
    pub command_mode: bool,
    /// Trigger+Shift dictates into the alternate output language.
    pub alternate_output: bool,
    pub paste_last: Option<Shortcut>,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            trigger: Shortcut::CTRL_WIN,
            space_for_hands_free: true,
            double_tap_for_hands_free: true,
            command_mode: true,
            alternate_output: true,
            paste_last: Some(Shortcut::ALT_SHIFT_Z),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Dictate,
    Command,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CancelReason {
    Escape,
    /// Another key was pressed right after starting, so it was probably a shortcut.
    OtherKey,
    /// The trigger was tapped rather than held.
    TooShort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start recording.
    Start {
        mode: Mode,
    },
    /// The user added Alt or Shift while holding the trigger.
    ModeChanged {
        mode: Mode,
    },
    /// The recording continues without holding the trigger.
    HandsFree,
    /// Stop recording and transcribe.
    Stop,
    Cancel {
        reason: CancelReason,
    },
    /// Inject [`vk::MASK`] so releasing Win doesn't open the Start menu.
    InjectMask,
    PasteLast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u16,
    pub down: bool,
    pub time_ms: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub swallow: bool,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Push-to-talk: recording while the trigger is held.
    Holding {
        start: u64,
        mode: Mode,
    },
    /// The trigger was released quickly. Recording continues briefly in case a
    /// second tap turns this into hands-free.
    TapPending {
        released: u64,
    },
    /// Recording without holding. `armed` turns true once the trigger has been
    /// released, after which pressing it again stops.
    HandsFree {
        armed: bool,
    },
    /// Nothing happens until the trigger is released.
    WaitRelease,
}

#[derive(Debug, Clone)]
pub struct GestureMachine {
    cfg: GestureConfig,
    state: State,
    down: Vec<u16>,
    swallowed: Vec<u16>,
    paused: bool,
}

impl GestureMachine {
    pub fn new(cfg: GestureConfig) -> Self {
        Self {
            cfg,
            state: State::Idle,
            down: Vec::with_capacity(8),
            swallowed: Vec::with_capacity(4),
            paused: false,
        }
    }

    pub fn config(&self) -> &GestureConfig {
        &self.cfg
    }

    pub fn set_config(&mut self, cfg: GestureConfig) {
        self.cfg = cfg;
        self.state = State::Idle;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.state = State::Idle;
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            State::Holding { .. } | State::TapPending { .. } | State::HandsFree { .. }
        )
    }

    /// Keys the machine believes are held, so the hook can re-check them
    /// against the real keyboard state after a missed release.
    pub fn held_keys(&self) -> &[u16] {
        &self.down
    }

    /// A hands-free dictation started without the keyboard, from the bar or
    /// the tray. The trigger stops it and Esc cancels it, as usual.
    pub fn begin_hands_free(&mut self) {
        if self.paused {
            return;
        }
        self.state = State::HandsFree {
            armed: !self.chord_held(),
        };
    }

    /// The session ended on its own (stop button, cancel button, time limit or error).
    pub fn session_ended(&mut self) {
        if self.is_active() {
            self.state = if self.chord_held() {
                State::WaitRelease
            } else {
                State::Idle
            };
        }
    }

    pub fn on_tick(&mut self, now: u64) -> Vec<Action> {
        if let State::TapPending { released, .. } = self.state
            && now.saturating_sub(released) >= DOUBLE_TAP_MS
        {
            self.state = State::Idle;
            return vec![Action::Cancel {
                reason: CancelReason::TooShort,
            }];
        }
        Vec::new()
    }

    pub fn on_key(&mut self, ev: KeyEvent) -> Outcome {
        let was_held = self.chord_held();
        let repeat = ev.down && self.down.contains(&ev.vk);
        if ev.down {
            if !repeat {
                self.down.push(ev.vk);
            }
        } else {
            self.down.retain(|k| *k != ev.vk);
        }
        let held = self.chord_held();

        let mut out = Outcome::default();

        // A key whose press we hid must have its release hidden too.
        if !ev.down {
            if let Some(i) = self.swallowed.iter().position(|k| *k == ev.vk) {
                self.swallowed.swap_remove(i);
                out.swallow = true;
            }
        } else if repeat && self.swallowed.contains(&ev.vk) {
            out.swallow = true;
        }

        if self.paused {
            return out;
        }

        if repeat {
            if self.is_trigger_key(ev.vk) {
                out.swallow = true;
            }
            return out;
        }

        match self.state {
            State::Idle => self.idle(ev, was_held, held, &mut out),
            State::Holding { start, mode } => self.holding(ev, start, mode, held, &mut out),
            State::TapPending { .. } => self.tap_pending(ev, was_held, held, &mut out),
            State::HandsFree { armed } => self.hands_free(ev, armed, was_held, held, &mut out),
            State::WaitRelease => {
                if ev.down && ev.vk == vk::SPACE && held {
                    self.swallow(ev.vk, &mut out);
                } else if self.is_trigger_key(ev.vk) {
                    out.swallow = true;
                }
                if !held {
                    self.state = State::Idle;
                }
            }
        }
        out
    }

    fn idle(&mut self, ev: KeyEvent, was_held: bool, held: bool, out: &mut Outcome) {
        if !ev.down {
            if self.is_trigger_key(ev.vk) {
                out.swallow = true;
            }
            return;
        }
        if held && !was_held {
            let mode = self.mode_from_held();
            if self.cfg.trigger.win || self.mod_down(Modifier::Win) {
                out.actions.push(Action::InjectMask);
            }
            out.actions.push(Action::Start { mode });
            if self.is_trigger_key(ev.vk) {
                self.swallow(ev.vk, out);
            }
            self.state = State::Holding {
                start: ev.time_ms,
                mode,
            };
            return;
        }
        if let Some(paste) = self.cfg.paste_last
            && paste.key == Some(ev.vk)
            && self.exact_mods(&paste)
        {
            self.swallow(ev.vk, out);
            out.actions.push(Action::PasteLast);
        }
    }

    fn holding(&mut self, ev: KeyEvent, start: u64, mode: Mode, held: bool, out: &mut Outcome) {
        if !held {
            if self.is_trigger_key(ev.vk) {
                out.swallow = true;
            }
            if ev.time_ms.saturating_sub(start) < MIN_HOLD_MS {
                if self.cfg.double_tap_for_hands_free {
                    self.state = State::TapPending {
                        released: ev.time_ms,
                    };
                } else {
                    self.state = State::Idle;
                    out.actions.push(Action::Cancel {
                        reason: CancelReason::TooShort,
                    });
                }
            } else {
                self.state = State::Idle;
                out.actions.push(Action::Stop);
            }
            return;
        }
        if !ev.down {
            return;
        }
        match ev.vk {
            vk::ESCAPE => {
                self.swallow(ev.vk, out);
                out.actions.push(Action::Cancel {
                    reason: CancelReason::Escape,
                });
                self.state = State::WaitRelease;
            }
            vk::SPACE if self.cfg.space_for_hands_free => {
                self.swallow(ev.vk, out);
                out.actions.push(Action::HandsFree);
                self.state = State::HandsFree { armed: false };
            }
            k if Modifier::of(k).is_some() => {
                let new_mode = self.mode_from_held();
                if new_mode != mode && rank(new_mode) > rank(mode) {
                    self.state = State::Holding {
                        start,
                        mode: new_mode,
                    };
                    out.actions.push(Action::ModeChanged { mode: new_mode });
                }
            }
            k if self.is_trigger_key(k) => {
                out.swallow = true;
            }
            _ => {
                if ev.time_ms.saturating_sub(start) < ACCIDENTAL_WINDOW_MS {
                    out.actions.push(Action::Cancel {
                        reason: CancelReason::OtherKey,
                    });
                    self.state = State::WaitRelease;
                }
            }
        }
    }

    fn tap_pending(&mut self, ev: KeyEvent, was_held: bool, held: bool, out: &mut Outcome) {
        if !ev.down {
            if self.is_trigger_key(ev.vk) {
                out.swallow = true;
            }
            return;
        }
        if held && !was_held {
            if self.cfg.trigger.win || self.mod_down(Modifier::Win) {
                out.actions.push(Action::InjectMask);
            }
            if self.is_trigger_key(ev.vk) {
                self.swallow(ev.vk, out);
            }
            out.actions.push(Action::HandsFree);
            self.state = State::HandsFree { armed: false };
            return;
        }
        if ev.vk == vk::ESCAPE {
            self.swallow(ev.vk, out);
            out.actions.push(Action::Cancel {
                reason: CancelReason::Escape,
            });
            self.state = State::Idle;
        } else if Modifier::of(ev.vk).is_none() {
            out.actions.push(Action::Cancel {
                reason: CancelReason::OtherKey,
            });
            self.state = State::Idle;
        }
    }

    fn hands_free(
        &mut self,
        ev: KeyEvent,
        armed: bool,
        was_held: bool,
        held: bool,
        out: &mut Outcome,
    ) {
        if !ev.down {
            if self.is_trigger_key(ev.vk) {
                out.swallow = true;
            }
            if !armed && !held {
                self.state = State::HandsFree { armed: true };
            }
            return;
        }
        if ev.vk == vk::ESCAPE {
            self.swallow(ev.vk, out);
            out.actions.push(Action::Cancel {
                reason: CancelReason::Escape,
            });
            self.state = if held {
                State::WaitRelease
            } else {
                State::Idle
            };
            return;
        }
        if armed && held && !was_held {
            if self.cfg.trigger.win || self.mod_down(Modifier::Win) {
                out.actions.push(Action::InjectMask);
            }
            if self.is_trigger_key(ev.vk) {
                self.swallow(ev.vk, out);
            }
            out.actions.push(Action::Stop);
            self.state = State::WaitRelease;
            return;
        }
        if ev.vk == vk::SPACE && held && self.cfg.space_for_hands_free {
            // Trigger+Space again toggles hands-free off.
            self.swallow(ev.vk, out);
            out.actions.push(Action::Stop);
            self.state = State::WaitRelease;
            return;
        }
        if self.is_trigger_key(ev.vk) {
            self.swallow(ev.vk, out);
        }
    }

    fn swallow(&mut self, vk: u16, out: &mut Outcome) {
        out.swallow = true;
        if !self.swallowed.contains(&vk) {
            self.swallowed.push(vk);
        }
    }

    fn mod_down(&self, m: Modifier) -> bool {
        self.down.iter().any(|k| Modifier::of(*k) == Some(m))
    }

    fn chord_held(&self) -> bool {
        let t = &self.cfg.trigger;
        for m in [
            Modifier::Ctrl,
            Modifier::Win,
            Modifier::Alt,
            Modifier::Shift,
        ] {
            if t.requires(m) && !self.mod_down(m) {
                return false;
            }
        }
        match t.key {
            Some(k) => self.down.contains(&k),
            None => t.ctrl || t.win || t.alt || t.shift,
        }
    }

    fn exact_mods(&self, s: &Shortcut) -> bool {
        [
            Modifier::Ctrl,
            Modifier::Win,
            Modifier::Alt,
            Modifier::Shift,
        ]
        .iter()
        .all(|m| s.requires(*m) == self.mod_down(*m))
    }

    fn is_trigger_key(&self, vk: u16) -> bool {
        self.cfg.trigger.key == Some(vk)
    }

    fn mode_from_held(&self) -> Mode {
        let t = &self.cfg.trigger;
        if self.cfg.command_mode && !t.alt && self.mod_down(Modifier::Alt) {
            Mode::Command
        } else if self.cfg.alternate_output && !t.shift && self.mod_down(Modifier::Shift) {
            Mode::Alternate
        } else {
            Mode::Dictate
        }
    }
}

fn rank(mode: Mode) -> u8 {
    match mode {
        Mode::Dictate => 0,
        Mode::Alternate => 1,
        Mode::Command => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: char = 'd';
    const U: char = 'u';

    struct Harness {
        m: GestureMachine,
        actions: Vec<Action>,
        swallowed: Vec<(u16, bool)>,
    }

    impl Harness {
        fn new() -> Self {
            Self::with(GestureConfig::default())
        }

        fn with(cfg: GestureConfig) -> Self {
            Self {
                m: GestureMachine::new(cfg),
                actions: Vec::new(),
                swallowed: Vec::new(),
            }
        }

        fn key(&mut self, vk: u16, dir: char, t: u64) -> bool {
            let out = self.m.on_key(KeyEvent {
                vk,
                down: dir == D,
                time_ms: t,
            });
            if out.swallow {
                self.swallowed.push((vk, dir == D));
            }
            self.actions.extend(out.actions);
            out.swallow
        }

        fn tick(&mut self, t: u64) {
            let a = self.m.on_tick(t);
            self.actions.extend(a);
        }

        fn take(&mut self) -> Vec<Action> {
            std::mem::take(&mut self.actions)
        }

        fn press_chord(&mut self, t: u64) {
            self.key(vk::LCONTROL, D, t);
            self.key(vk::LWIN, D, t + 10);
        }

        fn release_chord(&mut self, t: u64) {
            self.key(vk::LWIN, U, t);
            self.key(vk::LCONTROL, U, t + 5);
        }
    }

    #[test]
    fn hold_then_release_dictates() {
        let mut h = Harness::new();
        h.press_chord(0);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                }
            ]
        );
        h.release_chord(1_500);
        assert_eq!(h.take(), vec![Action::Stop]);
        assert!(h.swallowed.is_empty(), "modifiers are never swallowed");
    }

    #[test]
    fn order_of_modifiers_does_not_matter() {
        let mut h = Harness::new();
        h.key(vk::RWIN, D, 0);
        assert!(h.take().is_empty());
        h.key(vk::RCONTROL, D, 20);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                }
            ]
        );
        h.key(vk::RCONTROL, U, 900);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn short_tap_pastes_nothing() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        h.release_chord(200);
        assert!(h.take().is_empty(), "waits for a possible second tap");
        h.tick(300);
        assert!(h.take().is_empty());
        h.tick(200 + DOUBLE_TAP_MS);
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::TooShort
            }]
        );
    }

    #[test]
    fn short_tap_without_double_tap_cancels_at_once() {
        let mut h = Harness::with(GestureConfig {
            double_tap_for_hands_free: false,
            ..GestureConfig::default()
        });
        h.press_chord(0);
        h.take();
        h.release_chord(100);
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::TooShort
            }]
        );
    }

    #[test]
    fn double_tap_starts_hands_free_and_next_press_stops() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.release_chord(150);
        h.press_chord(300);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                },
                Action::InjectMask,
                Action::HandsFree
            ]
        );
        h.release_chord(400);
        h.tick(5_000);
        assert!(
            h.take().is_empty(),
            "hands-free keeps recording after release"
        );

        h.press_chord(9_000);
        assert_eq!(h.take(), vec![Action::InjectMask, Action::Stop]);
        h.release_chord(9_100);
        assert!(h.take().is_empty());

        h.press_chord(12_000);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                }
            ],
            "back to idle, a new hold starts a new dictation"
        );
    }

    #[test]
    fn second_tap_too_late_is_a_new_dictation() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.release_chord(100);
        h.tick(100 + DOUBLE_TAP_MS + 1);
        h.take();
        h.press_chord(1_000);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                }
            ]
        );
    }

    #[test]
    fn ctrl_win_space_starts_hands_free_and_swallows_space() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        assert!(h.key(vk::SPACE, D, 200));
        assert_eq!(h.take(), vec![Action::HandsFree]);
        assert!(h.key(vk::SPACE, U, 260), "the release is hidden too");
        h.release_chord(400);
        assert!(h.take().is_empty(), "releasing after Space doesn't stop");

        h.press_chord(5_000);
        assert_eq!(h.take(), vec![Action::InjectMask, Action::Stop]);
    }

    #[test]
    fn ctrl_win_space_again_stops_hands_free() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.key(vk::SPACE, D, 100);
        h.key(vk::SPACE, U, 150);
        h.take();
        assert!(
            h.key(vk::SPACE, D, 3_000),
            "chord still held, Space toggles off"
        );
        assert_eq!(h.take(), vec![Action::Stop]);
        h.key(vk::SPACE, U, 3_050);
        h.release_chord(3_200);
        assert!(h.take().is_empty());
    }

    #[test]
    fn space_repeat_while_held_stays_swallowed() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.key(vk::SPACE, D, 100);
        assert!(h.key(vk::SPACE, D, 600), "auto-repeat is hidden");
        assert!(h.key(vk::SPACE, U, 700));
    }

    #[test]
    fn escape_cancels_and_is_swallowed() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        assert!(h.key(vk::ESCAPE, D, 2_000));
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::Escape
            }]
        );
        assert!(h.key(vk::ESCAPE, U, 2_050));
        h.release_chord(2_300);
        assert!(
            h.take().is_empty(),
            "the release after cancelling does nothing"
        );
    }

    #[test]
    fn escape_cancels_hands_free() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.key(vk::SPACE, D, 100);
        h.key(vk::SPACE, U, 150);
        h.release_chord(300);
        h.take();
        assert!(h.key(vk::ESCAPE, D, 4_000));
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::Escape
            }]
        );
        assert!(h.key(vk::ESCAPE, U, 4_050));
        assert!(!h.key(vk::ESCAPE, D, 6_000), "Esc passes through when idle");
    }

    #[test]
    fn other_key_soon_after_start_cancels_and_passes_through() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        let d = 0x44;
        assert!(!h.key(d, D, 300), "Ctrl+Win+D still reaches Windows");
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::OtherKey
            }]
        );
        assert!(!h.key(d, U, 350));
        h.release_chord(500);
        assert!(h.take().is_empty());
    }

    #[test]
    fn other_key_late_in_a_dictation_is_ignored() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        assert!(!h.key(0x41, D, 3_000));
        h.key(0x41, U, 3_050);
        assert!(h.take().is_empty());
        h.release_chord(4_000);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn alt_switches_to_command_mode() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        assert!(!h.key(vk::LMENU, D, 200), "Alt itself is never swallowed");
        assert_eq!(
            h.take(),
            vec![Action::ModeChanged {
                mode: Mode::Command
            }]
        );
        h.key(vk::LMENU, U, 1_900);
        h.release_chord(2_000);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn alt_held_before_the_chord_starts_command_mode() {
        let mut h = Harness::new();
        h.key(vk::LMENU, D, 0);
        h.press_chord(10);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Command
                }
            ]
        );
    }

    #[test]
    fn shift_switches_to_alternate_output() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        h.key(vk::RSHIFT, D, 100);
        assert_eq!(
            h.take(),
            vec![Action::ModeChanged {
                mode: Mode::Alternate
            }]
        );
        h.key(vk::LMENU, D, 150);
        assert_eq!(
            h.take(),
            vec![Action::ModeChanged {
                mode: Mode::Command
            }]
        );
    }

    #[test]
    fn disabled_modes_stay_dictate() {
        let mut h = Harness::with(GestureConfig {
            command_mode: false,
            alternate_output: false,
            ..GestureConfig::default()
        });
        h.press_chord(0);
        h.take();
        h.key(vk::LMENU, D, 100);
        h.key(vk::LSHIFT, D, 120);
        assert!(h.take().is_empty());
    }

    #[test]
    fn key_repeat_does_not_restart() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        for t in (300..1_200).step_by(30) {
            h.key(vk::LWIN, D, t);
        }
        assert!(h.take().is_empty());
        h.release_chord(1_300);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn releasing_one_side_keeps_the_chord_when_the_other_side_is_held() {
        let mut h = Harness::new();
        h.key(vk::LCONTROL, D, 0);
        h.key(vk::RCONTROL, D, 5);
        h.key(vk::LWIN, D, 10);
        h.take();
        h.key(vk::LCONTROL, U, 800);
        assert!(h.take().is_empty());
        h.key(vk::RCONTROL, U, 900);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn paste_last_shortcut() {
        let mut h = Harness::new();
        h.key(vk::LMENU, D, 0);
        h.key(vk::LSHIFT, D, 10);
        assert!(h.key(0x5A, D, 20));
        assert_eq!(h.take(), vec![Action::PasteLast]);
        assert!(h.key(0x5A, U, 60));
        h.key(vk::LSHIFT, U, 80);
        h.key(vk::LMENU, U, 90);

        h.key(vk::LCONTROL, D, 100);
        h.key(vk::LMENU, D, 110);
        h.key(vk::LSHIFT, D, 120);
        assert!(!h.key(0x5A, D, 130), "extra modifiers don't match");
        assert!(h.take().is_empty());
    }

    #[test]
    fn caps_lock_trigger_is_swallowed() {
        let mut h = Harness::with(GestureConfig {
            trigger: Shortcut {
                ctrl: false,
                win: false,
                alt: false,
                shift: false,
                key: Some(vk::CAPITAL),
            },
            ..GestureConfig::default()
        });
        assert!(h.key(vk::CAPITAL, D, 0));
        assert_eq!(
            h.take(),
            vec![Action::Start {
                mode: Mode::Dictate
            }]
        );
        assert!(h.key(vk::CAPITAL, D, 500), "repeat swallowed");
        assert!(h.key(vk::CAPITAL, U, 1_000));
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn mouse_button_trigger_double_tap() {
        let mut h = Harness::with(GestureConfig {
            trigger: Shortcut {
                ctrl: false,
                win: false,
                alt: false,
                shift: false,
                key: Some(vk::XBUTTON2),
            },
            ..GestureConfig::default()
        });
        h.key(vk::XBUTTON2, D, 0);
        h.key(vk::XBUTTON2, U, 100);
        h.key(vk::XBUTTON2, D, 250);
        assert_eq!(
            h.take(),
            vec![
                Action::Start {
                    mode: Mode::Dictate
                },
                Action::HandsFree
            ]
        );
        h.key(vk::XBUTTON2, U, 300);
        h.key(vk::XBUTTON2, D, 3_000);
        assert_eq!(h.take(), vec![Action::Stop]);
    }

    #[test]
    fn session_ended_externally_returns_to_idle() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.key(vk::SPACE, D, 100);
        h.key(vk::SPACE, U, 150);
        h.release_chord(300);
        h.take();
        h.m.session_ended();
        assert!(!h.m.is_active());
        h.press_chord(2_000);
        assert_eq!(
            h.take(),
            vec![
                Action::InjectMask,
                Action::Start {
                    mode: Mode::Dictate
                }
            ]
        );
    }

    #[test]
    fn hands_free_from_the_bar_obeys_the_keyboard() {
        let mut h = Harness::new();
        h.m.begin_hands_free();
        assert!(h.m.is_active());
        h.press_chord(1_000);
        assert_eq!(h.take(), vec![Action::InjectMask, Action::Stop]);
        h.release_chord(1_200);
        assert!(h.take().is_empty());

        h.m.begin_hands_free();
        assert!(h.key(vk::ESCAPE, D, 2_000));
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::Escape
            }]
        );
        assert!(!h.m.is_active());
    }

    #[test]
    fn session_ended_while_chord_held_waits_for_release() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.take();
        h.m.session_ended();
        h.release_chord(3_000);
        assert!(h.take().is_empty());
    }

    #[test]
    fn paused_machine_does_nothing() {
        let mut h = Harness::new();
        h.m.set_paused(true);
        h.press_chord(0);
        h.key(vk::SPACE, D, 100);
        assert!(h.take().is_empty());
        assert!(h.swallowed.is_empty());
    }

    #[test]
    fn other_key_during_tap_pending_cancels() {
        let mut h = Harness::new();
        h.press_chord(0);
        h.release_chord(100);
        h.take();
        assert!(!h.key(0x41, D, 200));
        assert_eq!(
            h.take(),
            vec![Action::Cancel {
                reason: CancelReason::OtherKey
            }]
        );
    }

    #[test]
    fn shortcut_labels() {
        #[cfg(target_os = "macos")]
        assert_eq!(Shortcut::CTRL_WIN.label(), "Ctrl+Cmd");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(Shortcut::CTRL_WIN.label(), "Ctrl+Win");

        #[cfg(target_os = "macos")]
        assert_eq!(Shortcut::ALT_SHIFT_Z.label(), "Option+Shift+Z");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(Shortcut::ALT_SHIFT_Z.label(), "Alt+Shift+Z");
        let f13 = Shortcut {
            ctrl: false,
            win: false,
            alt: false,
            shift: false,
            key: Some(0x7C),
        };
        assert_eq!(f13.label(), "F13");
        assert_eq!(
            Shortcut {
                key: Some(vk::XBUTTON2),
                ..f13
            }
            .label(),
            "Mouse 5"
        );
    }

    #[test]
    fn trigger_validation() {
        assert!(Shortcut::CTRL_WIN.is_valid_trigger());
        let letter = Shortcut {
            ctrl: false,
            win: false,
            alt: false,
            shift: false,
            key: Some(0x41),
        };
        assert!(!letter.is_valid_trigger());
        let f13 = Shortcut {
            key: Some(0x7C),
            ..letter
        };
        assert!(f13.is_valid_trigger());
        let none = Shortcut {
            key: None,
            ..letter
        };
        assert!(!none.is_valid_trigger());
    }
}
