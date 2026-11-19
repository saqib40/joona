//! Global dictation shortcuts.
//!
//! [`gesture`] holds the platform-independent state machine. The Windows
//! implementation feeds it from a `WH_KEYBOARD_LL` hook. Other platforms get a
//! stub until they have their own hook.

pub mod gesture;
mod record;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
pub mod macos;

pub use gesture::{Action, CancelReason, GestureConfig, Mode, Shortcut};
pub use record::Recorded;
use tokio::sync::mpsc::UnboundedSender;

pub struct Hotkeys {
    #[cfg(windows)]
    inner: windows::HookThread,
    #[cfg(target_os = "macos")]
    inner: macos::HookThread,
}

impl Hotkeys {
    pub fn start(cfg: GestureConfig, tx: UnboundedSender<Action>) -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: windows::HookThread::start(cfg, tx)?,
            })
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                inner: macos::HookThread::start(cfg, tx)?,
            })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = (cfg, tx);
            Ok(Self {})
        }
    }

    pub fn update_config(&self, cfg: GestureConfig) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.update_config(cfg);
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = cfg;
    }

    pub fn set_paused(&self, paused: bool) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.set_paused(paused);
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = paused;
    }

    /// Tells the gesture machine that the session finished without a gesture,
    /// for example from the stop button or the time limit.
    pub fn session_ended(&self) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.session_ended();
    }

    /// Tells the gesture machine about a hands-free dictation started from
    /// the bar or the tray, so the shortcut and Esc can end it.
    pub fn begin_hands_free(&self) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.begin_hands_free();
    }

    /// Records a new shortcut: until everything is released or Esc is
    /// pressed, keys go to `tx` instead of the gesture machine.
    pub fn record(&self, tx: UnboundedSender<Recorded>) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.record(tx);
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = tx.send(Recorded::Cancelled);
    }

    pub fn stop_recording(&self) {
        #[cfg(any(windows, target_os = "macos"))]
        self.inner.stop_recording();
    }
}

/// Whether every modifier key is released.
pub fn modifiers_released() -> bool {
    #[cfg(windows)]
    {
        windows::modifiers_released()
    }
    #[cfg(target_os = "macos")]
    {
        macos::modifiers_released()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        true
    }
}

#[cfg(windows)]
pub use windows::JOONA_INPUT_MARKER;
#[cfg(target_os = "macos")]
pub use macos::JOONA_INPUT_MARKER;
#[cfg(not(any(windows, target_os = "macos")))]
pub const JOONA_INPUT_MARKER: usize = 0x4A4F_4F4E;
