//! Puts dictated text into the app the user is typing in.
//!
//! Pasting goes through the clipboard: Joona snapshots it, offers the text
//! with delayed rendering (marked to stay out of clipboard history and cloud
//! clipboard), sends Ctrl+V, and restores the snapshot once the app has read
//! the text. If nothing reads it, the paste didn't happen, so the text is left
//! on the clipboard for the user to paste by hand. Typing sends the text as
//! Unicode key presses instead, for apps where Ctrl+V doesn't paste.

#[cfg(windows)]
mod windows;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(windows)]
pub use windows::{Inserter, foreground};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    #[default]
    Paste,
    Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct InsertSettings {
    pub method: Method,
    /// Put the previous clipboard contents back after pasting.
    pub restore_clipboard: bool,
    /// End each dictation with a space, so the next one doesn't run into it.
    pub trailing_space: bool,
    /// How long an app gets to read pasted text before Joona leaves it on
    /// the clipboard instead.
    pub paste_timeout_ms: u32,
    /// Extra time an app gets to finish pasting before the clipboard is restored.
    pub restore_delay_ms: u32,
    pub overrides: Vec<AppOverride>,
}

impl Default for InsertSettings {
    fn default() -> Self {
        Self {
            method: Method::Paste,
            restore_clipboard: true,
            trailing_space: true,
            paste_timeout_ms: 1_500,
            restore_delay_ms: 150,
            overrides: Vec::new(),
        }
    }
}

/// Insertion settings for one app, matched by its executable name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppOverride {
    /// For example `mintty.exe`. Matched without regard to case.
    pub exe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<Method>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_timeout_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_delay_ms: Option<u32>,
}

/// Apps known to need something other than the defaults. The user's own
/// overrides win over these.
fn built_in_overrides() -> Vec<AppOverride> {
    let typed = |exe: &str| AppOverride {
        exe: exe.into(),
        method: Some(Method::Type),
        paste_timeout_ms: None,
        restore_delay_ms: None,
    };
    let slow = |exe: &str| AppOverride {
        exe: exe.into(),
        method: None,
        paste_timeout_ms: Some(5_000),
        restore_delay_ms: Some(500),
    };
    vec![
        // Terminals where Ctrl+V reaches the shell as a control character.
        typed("mintty.exe"),
        typed("putty.exe"),
        typed("kitty.exe"),
        // Remote sessions read the clipboard over the network.
        slow("mstsc.exe"),
        slow("msrdc.exe"),
        slow("vmconnect.exe"),
        slow("vmware.exe"),
        slow("virtualboxvm.exe"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertOptions {
    pub method: Method,
    pub restore_clipboard: bool,
    pub paste_timeout: Duration,
    pub restore_delay: Duration,
}

impl InsertSettings {
    /// The options for a dictation into `exe`.
    pub fn options_for(&self, exe: Option<&str>) -> InsertOptions {
        let mut options = InsertOptions {
            method: self.method,
            restore_clipboard: self.restore_clipboard,
            paste_timeout: Duration::from_millis(self.paste_timeout_ms.into()),
            restore_delay: Duration::from_millis(self.restore_delay_ms.into()),
        };
        let Some(exe) = exe else {
            return options;
        };
        let built_in = built_in_overrides();
        let matching = built_in
            .iter()
            .chain(&self.overrides)
            .filter(|o| o.exe.eq_ignore_ascii_case(exe));
        for o in matching {
            if let Some(method) = o.method {
                options.method = method;
            }
            if let Some(ms) = o.paste_timeout_ms {
                options.paste_timeout = Duration::from_millis(ms.into());
            }
            if let Some(ms) = o.restore_delay_ms {
                options.restore_delay = Duration::from_millis(ms.into());
            }
        }
        options
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum Outcome {
    Pasted,
    Typed,
    /// The text is on the clipboard for the user to paste with Ctrl+V.
    Copied(CopyReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CopyReason {
    /// The app runs as administrator, and Windows blocks input into it.
    Elevated,
    /// No app with a text field is in front, for example the desktop.
    NoTarget,
    /// The app isn't responding.
    Unresponsive,
    /// The user is still holding modifier keys.
    KeysHeld,
    /// The app didn't read the clipboard after Ctrl+V.
    NotAccepted,
}

impl CopyReason {
    /// Why Joona couldn't press keys in the app in front.
    pub fn explain(self) -> &'static str {
        match self {
            CopyReason::Elevated => {
                "The app runs as administrator, so Windows won't let Joona type into it."
            }
            CopyReason::NoTarget => "Click into a text field first, then dictate.",
            CopyReason::Unresponsive => "The app isn't responding.",
            CopyReason::KeysHeld => "Some keys were still held down.",
            CopyReason::NotAccepted => "The app didn't accept the paste.",
        }
    }
}

/// The app in front when the text is inserted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundApp {
    /// Lower-case executable name, for example `slack.exe`.
    pub exe: String,
    pub title: String,
    pub pid: u32,
    /// Runs with higher privileges than Joona, so input into it is blocked.
    pub elevated: bool,
    /// One of Joona's own windows, such as onboarding's practice box.
    pub own: bool,
    #[serde(skip)]
    pub(crate) class: String,
    #[serde(skip)]
    pub(crate) hung: bool,
}

impl ForegroundApp {
    /// The desktop and the taskbar have nowhere to type.
    pub fn is_shell(&self) -> bool {
        matches!(
            self.class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        )
    }
}

/// Why insertion can't even be attempted in this app, if it can't.
pub(crate) fn blocked(app: Option<&ForegroundApp>) -> Option<CopyReason> {
    let Some(app) = app else {
        return Some(CopyReason::NoTarget);
    };
    if app.elevated {
        Some(CopyReason::Elevated)
    } else if app.is_shell() {
        Some(CopyReason::NoTarget)
    } else if app.hung {
        Some(CopyReason::Unresponsive)
    } else {
        None
    }
}

/// What delayed rendering told us after Ctrl+V was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PasteSignal {
    /// The app asked for the text after Ctrl+V: it pasted.
    ReadAfterPaste,
    /// Something, usually a clipboard manager, asked before Ctrl+V was sent,
    /// so the app's own read can't be seen.
    ReadBeforePaste,
    /// Nobody asked: the paste didn't happen.
    NotRead,
}

/// How long a paste is assumed to take when it can't be observed.
pub(crate) const UNOBSERVED_PASTE: Duration = Duration::from_millis(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterPaste {
    /// Wait this long, then restore the snapshot.
    Restore(Duration),
    /// Leave the text on the clipboard.
    KeepText,
}

pub(crate) fn after_paste(signal: PasteSignal, options: &InsertOptions) -> (Outcome, AfterPaste) {
    match signal {
        PasteSignal::ReadAfterPaste => {
            (Outcome::Pasted, AfterPaste::Restore(options.restore_delay))
        }
        PasteSignal::ReadBeforePaste => (
            Outcome::Pasted,
            AfterPaste::Restore(UNOBSERVED_PASTE.max(options.restore_delay)),
        ),
        PasteSignal::NotRead => (
            Outcome::Copied(CopyReason::NotAccepted),
            AfterPaste::KeepText,
        ),
    }
}

/// One simulated key press when typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// A UTF-16 code unit sent as a Unicode key press.
    Unit(u16),
    /// Shift+Enter starts a new line without sending a chat message.
    NewLine,
}

pub(crate) fn strokes(text: &str) -> Vec<Stroke> {
    let mut out = Vec::with_capacity(text.len());
    let mut units = [0u16; 2];
    for ch in text.chars() {
        match ch {
            '\r' => {}
            '\n' => out.push(Stroke::NewLine),
            _ => out.extend(ch.encode_utf16(&mut units).iter().map(|&u| Stroke::Unit(u))),
        }
    }
    out
}

#[cfg(not(windows))]
pub struct Inserter;

#[cfg(not(windows))]
impl Inserter {
    pub fn new() -> anyhow::Result<Self> {
        anyhow::bail!("text insertion isn't available on this platform yet")
    }

    pub fn insert(&self, _text: &str, _options: &InsertOptions) -> anyhow::Result<Outcome> {
        anyhow::bail!("text insertion isn't available on this platform yet")
    }

    pub fn press_enter(&self) -> anyhow::Result<()> {
        anyhow::bail!("pressing keys isn't available on this platform yet")
    }

    pub fn copy(&self, _text: &str) -> anyhow::Result<()> {
        anyhow::bail!("the clipboard isn't available on this platform yet")
    }

    pub fn clipboard_text(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }

    pub fn copy_selection(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

#[cfg(not(windows))]
pub fn foreground() -> Option<ForegroundApp> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(exe: &str) -> ForegroundApp {
        ForegroundApp {
            exe: exe.into(),
            title: String::new(),
            pid: 1,
            elevated: false,
            own: false,
            class: "Chrome_WidgetWin_1".into(),
            hung: false,
        }
    }

    #[test]
    fn defaults_paste_and_restore() {
        let options = InsertSettings::default().options_for(Some("notepad.exe"));
        assert_eq!(options.method, Method::Paste);
        assert!(options.restore_clipboard);
        assert_eq!(options.paste_timeout, Duration::from_millis(1_500));
    }

    #[test]
    fn built_in_overrides_apply_without_regard_to_case() {
        let settings = InsertSettings::default();
        assert_eq!(
            settings.options_for(Some("MinTTY.exe")).method,
            Method::Type
        );
        let rdp = settings.options_for(Some("mstsc.exe"));
        assert_eq!(rdp.method, Method::Paste);
        assert_eq!(rdp.paste_timeout, Duration::from_secs(5));
    }

    #[test]
    fn user_overrides_win_over_built_in_ones() {
        let settings = InsertSettings {
            overrides: vec![AppOverride {
                exe: "mintty.exe".into(),
                method: Some(Method::Paste),
                paste_timeout_ms: None,
                restore_delay_ms: Some(300),
            }],
            ..Default::default()
        };
        let options = settings.options_for(Some("mintty.exe"));
        assert_eq!(options.method, Method::Paste);
        assert_eq!(options.restore_delay, Duration::from_millis(300));
    }

    #[test]
    fn insertion_is_blocked_where_it_cannot_work() {
        assert_eq!(blocked(None), Some(CopyReason::NoTarget));
        assert_eq!(blocked(Some(&app("slack.exe"))), None);

        let admin = ForegroundApp {
            elevated: true,
            ..app("regedit.exe")
        };
        assert_eq!(blocked(Some(&admin)), Some(CopyReason::Elevated));

        let desktop = ForegroundApp {
            class: "Progman".into(),
            ..app("explorer.exe")
        };
        assert_eq!(blocked(Some(&desktop)), Some(CopyReason::NoTarget));

        let frozen = ForegroundApp {
            hung: true,
            ..app("winword.exe")
        };
        assert_eq!(blocked(Some(&frozen)), Some(CopyReason::Unresponsive));
    }

    #[test]
    fn the_clipboard_is_restored_only_after_a_paste() {
        let options = InsertSettings::default().options_for(None);
        assert_eq!(
            after_paste(PasteSignal::ReadAfterPaste, &options),
            (
                Outcome::Pasted,
                AfterPaste::Restore(Duration::from_millis(150))
            )
        );
        assert_eq!(
            after_paste(PasteSignal::ReadBeforePaste, &options),
            (Outcome::Pasted, AfterPaste::Restore(UNOBSERVED_PASTE))
        );
        assert_eq!(
            after_paste(PasteSignal::NotRead, &options),
            (
                Outcome::Copied(CopyReason::NotAccepted),
                AfterPaste::KeepText
            )
        );
    }

    #[test]
    fn typing_splits_text_into_key_presses() {
        assert_eq!(
            strokes("a\r\nb"),
            vec![
                Stroke::Unit('a' as u16),
                Stroke::NewLine,
                Stroke::Unit('b' as u16)
            ]
        );
        // Devanagari stays one unit per character; emoji need a surrogate pair.
        assert_eq!(strokes("नम").len(), 2);
        assert_eq!(
            strokes("👍"),
            vec![Stroke::Unit(0xD83D), Stroke::Unit(0xDC4D)]
        );
    }

    #[test]
    fn outcomes_serialize_for_the_ui() {
        let json = serde_json::to_value(Outcome::Copied(CopyReason::Elevated)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "copied", "reason": "elevated"})
        );
        let json = serde_json::to_value(Outcome::Pasted).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "pasted"}));
    }
}
