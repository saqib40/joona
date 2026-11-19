//! Records a shortcut for Settings from the keys the user presses.
//!
//! While recording, the hook swallows every key, so pressing Win doesn't open
//! Start and the current trigger doesn't start a dictation.

use super::gesture::{Modifier, Shortcut, vk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// The combination held so far.
    Preview(Shortcut),
    /// Everything was released; this is the new shortcut.
    Done(Shortcut),
    /// Esc on its own.
    Cancelled,
}

const NONE: Shortcut = Shortcut {
    ctrl: false,
    win: false,
    alt: false,
    shift: false,
    key: None,
};

#[derive(Debug, Default)]
pub struct Recorder {
    held: Vec<u16>,
    chord: Option<Shortcut>,
}

impl Recorder {
    pub fn on_key(&mut self, vk_code: u16, down: bool) -> Option<Recorded> {
        if !down {
            // A key that was already down when recording began ends nothing.
            if !self.held.contains(&vk_code) {
                return None;
            }
            self.held.retain(|k| *k != vk_code);
            return match self.held.is_empty() {
                true => self.chord.take().map(Recorded::Done),
                false => None,
            };
        }
        if self.held.contains(&vk_code) {
            return None;
        }
        if vk_code == vk::ESCAPE && self.held.is_empty() {
            return Some(Recorded::Cancelled);
        }
        self.held.push(vk_code);
        let chord = self.chord.get_or_insert(NONE);
        match Modifier::of(vk_code) {
            Some(Modifier::Ctrl) => chord.ctrl = true,
            Some(Modifier::Win) => chord.win = true,
            Some(Modifier::Alt) => chord.alt = true,
            Some(Modifier::Shift) => chord.shift = true,
            None => chord.key = Some(vk_code),
        }
        Some(Recorded::Preview(*chord))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(r: &mut Recorder, keys: &[(u16, bool)]) -> Vec<Recorded> {
        keys.iter()
            .filter_map(|(k, down)| r.on_key(*k, *down))
            .collect()
    }

    #[test]
    fn records_modifiers_on_their_own() {
        let mut r = Recorder::default();
        let out = press(
            &mut r,
            &[
                (vk::LCONTROL, true),
                (vk::LWIN, true),
                (vk::LWIN, true),
                (vk::LCONTROL, false),
                (vk::LWIN, false),
            ],
        );
        assert_eq!(out.len(), 3, "auto-repeat isn't reported: {out:?}");
        assert_eq!(out[2], Recorded::Done(Shortcut::CTRL_WIN));
    }

    #[test]
    fn records_a_key_with_modifiers_and_ignores_stale_releases() {
        let mut r = Recorder::default();
        let out = press(
            &mut r,
            &[
                (0x0D, false),
                (vk::LMENU, true),
                (vk::LSHIFT, true),
                (0x5A, true),
                (0x5A, false),
                (vk::LSHIFT, false),
                (vk::LMENU, false),
            ],
        );
        assert_eq!(out.last(), Some(&Recorded::Done(Shortcut::ALT_SHIFT_Z)));
    }

    #[test]
    fn escape_cancels_but_can_be_part_of_a_chord() {
        let mut r = Recorder::default();
        assert_eq!(r.on_key(vk::ESCAPE, true), Some(Recorded::Cancelled));
        let mut r = Recorder::default();
        r.on_key(vk::LCONTROL, true);
        assert!(
            matches!(r.on_key(vk::ESCAPE, true), Some(Recorded::Preview(s)) if s.key == Some(vk::ESCAPE))
        );
    }

    #[test]
    fn mouse_buttons_work_like_keys() {
        let mut r = Recorder::default();
        r.on_key(vk::XBUTTON2, true);
        let Some(Recorded::Done(s)) = r.on_key(vk::XBUTTON2, false) else {
            panic!("expected a shortcut");
        };
        assert_eq!(s.key, Some(vk::XBUTTON2));
        assert!(s.is_valid_trigger());
    }
}
