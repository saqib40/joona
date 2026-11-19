mod clipboard;
mod foreground;
mod input;

use std::time::{Duration, Instant};

pub use foreground::foreground;

use self::clipboard::Clipboard;
use super::{
    AfterPaste, CopyReason, InsertOptions, Method, Outcome, PasteSignal, after_paste, blocked,
};

/// How long the user gets to let go of modifier keys before Joona gives up on simulating a shortcut.
const MODIFIER_WAIT: Duration = Duration::from_secs(1);
/// How long an app gets to put its selection on the clipboard after Cmd+C.
const SELECTION_WAIT: Duration = Duration::from_millis(400);

pub struct Inserter {
    clipboard: Clipboard,
}

impl Inserter {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            clipboard: Clipboard::new()?,
        })
    }

    /// Inserts `text` into the app in front.
    pub fn insert(&self, text: &str, options: &InsertOptions) -> anyhow::Result<Outcome> {
        let target = foreground();
        if let Some(reason) = blocked(target.as_ref()) {
            self.clipboard.set_text(text, true)?;
            return Ok(Outcome::Copied(reason));
        }
        if !input::wait_for_modifiers(MODIFIER_WAIT) {
            self.clipboard.set_text(text, true)?;
            return Ok(Outcome::Copied(CopyReason::KeysHeld));
        }
        match options.method {
            Method::Type => {
                input::type_text(text)?;
                Ok(Outcome::Typed)
            }
            Method::Paste => self.paste(text, options),
        }
    }

    fn paste(&self, text: &str, options: &InsertOptions) -> anyhow::Result<Outcome> {
        let prepared = (|| {
            let snapshot = match options.restore_clipboard {
                true => Some(self.clipboard.snapshot()?),
                false => None,
            };
            self.clipboard.offer(text)?;
            anyhow::Ok(snapshot)
        })();

        let snapshot = match prepared {
            Ok(snapshot) => snapshot,
            Err(e) => {
                tracing::warn!("clipboard unavailable, typing instead: {e:#}");
                input::type_text(text)?;
                return Ok(Outcome::Typed);
            }
        };

        let sent = Instant::now();
        if let Err(e) = input::paste() {
            tracing::warn!("couldn't send Cmd+V: {e:#}");
            self.clipboard.render_now()?;
            return Ok(Outcome::Copied(CopyReason::NotAccepted));
        }

        let signal = match self.clipboard.wait_rendered(options.paste_timeout) {
            Some(read) if read >= sent => PasteSignal::ReadAfterPaste,
            Some(_) => PasteSignal::ReadBeforePaste,
            None => PasteSignal::NotRead,
        };
        let (outcome, next) = after_paste(signal, options);
        tracing::debug!(
            ?signal,
            ?outcome,
            waited_ms = sent.elapsed().as_millis() as u64,
            "paste"
        );

        match next {
            AfterPaste::Restore(delay) => {
                if let Some(snapshot) = snapshot {
                    std::thread::sleep(delay);
                    if !self.clipboard.restore(snapshot, false)? {
                        tracing::debug!("something else replaced the clipboard; not restoring it");
                    }
                }
            }
            AfterPaste::KeepText => self.clipboard.render_now()?,
        }

        Ok(outcome)
    }

    /// Presses Enter in the app in front.
    pub fn press_enter(&self) -> anyhow::Result<()> {
        if let Some(reason) = blocked(foreground().as_ref()) {
            anyhow::bail!(reason.explain());
        }
        if !input::wait_for_modifiers(MODIFIER_WAIT) {
            anyhow::bail!(CopyReason::KeysHeld.explain());
        }
        input::press_enter()
    }

    /// Copies `text` like Cmd+C would, history included.
    pub fn copy(&self, text: &str) -> anyhow::Result<()> {
        self.clipboard.set_text(text, false)
    }

    pub fn clipboard_text(&self) -> anyhow::Result<Option<String>> {
        self.clipboard.text()
    }

    /// The text selected in the app in front, read with a simulated Cmd+C.
    /// The clipboard is restored afterwards.
    pub fn copy_selection(&self) -> anyhow::Result<Option<String>> {
        if blocked(foreground().as_ref()).is_some() || !input::wait_for_modifiers(MODIFIER_WAIT) {
            return Ok(None);
        }
        let snapshot = self.clipboard.snapshot()?;
        let before = Clipboard::sequence();
        input::copy()?;
        let deadline = Instant::now() + SELECTION_WAIT;
        while Clipboard::sequence() == before && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if Clipboard::sequence() == before {
            // Nothing was selected, so nothing was copied.
            return Ok(None);
        }
        let text = self.clipboard.copied_selection();
        self.clipboard.restore(snapshot, true)?;
        Ok(text?.filter(|t| !t.trim().is_empty()))
    }
}
