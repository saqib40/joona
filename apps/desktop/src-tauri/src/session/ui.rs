//! What the session shows: the bar's state, toasts and sounds.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::history::Entry;
use crate::hotkeys::Mode;
use crate::insert::Outcome;
use crate::router::RouteLabel;
use crate::sounds::Cue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum BarState {
    Resting,
    Listening(Listening),
    Processing(Processing),
    /// Briefly, after the text went in.
    Done(Done),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Listening {
    pub mode: Mode,
    pub hands_free: bool,
    /// How long the dictation had been running when this state was sent.
    pub elapsed_ms: u32,
    pub route: RouteLabel,
    /// Less than a minute left before the dictation stops on its own.
    pub ending_soon: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Processing {
    pub route: RouteLabel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Error,
}

/// What a toast button does. The bar sends it back with the toast's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ToastAction {
    /// Transcribe a cancelled dictation after all.
    Undo,
    /// Transcribe a failed dictation again.
    Retry,
    /// Transcribe a failed cloud dictation on this PC.
    UseLocal,
    /// Always type into this app instead of pasting.
    AlwaysType,
    OpenModels,
    OpenLanguages,
    OpenProviders,
    OpenMicSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToastButton {
    pub action: ToastAction,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    pub id: u32,
    pub tone: Tone,
    pub title: String,
    pub detail: Option<String>,
    pub actions: Vec<ToastButton>,
    /// Hide after this long. Hovering keeps it open.
    pub timeout_ms: u32,
}

impl Toast {
    pub fn new(tone: Tone, title: impl Into<String>) -> Self {
        Self {
            id: 0,
            tone,
            title: title.into(),
            detail: None,
            actions: Vec::new(),
            timeout_ms: match tone {
                Tone::Error => 8_000,
                Tone::Warning => 6_000,
                _ => 4_000,
            },
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn action(mut self, action: ToastAction, label: impl Into<String>) -> Self {
        self.actions.push(ToastButton {
            action,
            label: label.into(),
        });
        self
    }

    pub fn timeout(mut self, ms: u32) -> Self {
        self.timeout_ms = ms;
        self
    }
}

/// The parts of the app a session drives. The Tauri app implements it; tests
/// can record calls instead.
pub trait Ui: Send + Sync + 'static {
    fn bar(&self, state: &BarState);
    /// Microphone level from 0 to 1, about 30 times a second while listening.
    fn level(&self, level: f32);
    fn toast(&self, toast: &Toast);
    fn dismiss_toast(&self, id: u32);
    fn sound(&self, cue: Cue);
    /// Opens the Hub, optionally on a page such as `models`.
    fn open_hub(&self, page: Option<&str>);
    /// Opens the microphone privacy switches in Windows Settings.
    fn open_mic_privacy(&self);
    /// A dictation was added to the history.
    fn history(&self, entry: &Entry);
}
