//! The microphone test in Settings: live levels without dictating.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use super::capture::{AudioSink, Capture};

#[derive(Default)]
pub struct MicTest {
    current: Mutex<Option<(u32, Capture)>>,
    next: AtomicU32,
}

struct LevelSink<F>(F);

impl<F: FnMut(f32) + Send + 'static> AudioSink for LevelSink<F> {
    fn on_audio(&mut self, _samples: &[f32]) {}

    fn on_level(&mut self, level: f32) {
        (self.0)(level);
    }

    fn on_error(&mut self, message: String) {
        tracing::warn!("microphone test: {message}");
    }
}

impl MicTest {
    /// Opens the microphone and calls `level` about 30 times a second, until
    /// [`MicTest::stop`]. Returns the test's token and the device's name.
    pub fn start(
        &self,
        device: Option<&str>,
        level: impl FnMut(f32) + Send + 'static,
    ) -> anyhow::Result<(u32, String)> {
        self.stop(None);
        let capture = Capture::start(device, Box::new(LevelSink(level)))?;
        let name = capture.info.device_name.clone();
        let token = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        *self.current.lock().unwrap_or_else(|e| e.into_inner()) = Some((token, capture));
        Ok((token, name))
    }

    /// Stops the test with this token, or any test with `None`.
    pub fn stop(&self, token: Option<u32>) {
        let taken = {
            let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
            match (&*current, token) {
                (Some((running, _)), Some(token)) if *running != token => None,
                _ => current.take(),
            }
        };
        if let Some((_, capture)) = taken {
            capture.stop();
        }
    }
}
