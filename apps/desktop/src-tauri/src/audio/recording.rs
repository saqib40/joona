//! The audio of one dictation. It only ever lives in memory.

use super::{SAMPLE_RATE, samples_to_ms};

/// Longest dictation. The bar warns a minute before it stops.
pub const MAX_RECORDING_MS: u64 = 20 * 60 * 1000;
pub const WARN_RECORDING_MS: u64 = 19 * 60 * 1000;
const MAX_SAMPLES: usize = (MAX_RECORDING_MS * SAMPLE_RATE as u64 / 1000) as usize;
/// A muted or disconnected microphone delivers digital silence or faint hiss
/// below this peak (about -54 dBFS). Real rooms are louder than this.
const MUTED_PEAK: f32 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    None,
    /// Crossed [`WARN_RECORDING_MS`] with this push.
    Warn,
    /// At [`MAX_RECORDING_MS`]; further audio is dropped.
    Reached,
}

pub struct Recording {
    samples: Vec<f32>,
    streamed: usize,
    peak: f32,
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

impl Recording {
    pub fn new() -> Self {
        Self {
            samples: Vec::with_capacity(SAMPLE_RATE as usize * 30),
            streamed: 0,
            peak: 0.0,
        }
    }

    pub fn push(&mut self, audio: &[f32]) -> Limit {
        let before = self.duration_ms();
        let room = MAX_SAMPLES.saturating_sub(self.samples.len());
        let kept = &audio[..audio.len().min(room)];
        self.peak = kept.iter().fold(self.peak, |m, s| m.max(s.abs()));
        self.samples.extend_from_slice(kept);
        let after = self.duration_ms();
        if self.samples.len() >= MAX_SAMPLES {
            Limit::Reached
        } else if before < WARN_RECORDING_MS && after >= WARN_RECORDING_MS {
            Limit::Warn
        } else {
            Limit::None
        }
    }

    pub fn duration_ms(&self) -> u64 {
        samples_to_ms(self.samples.len())
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }

    /// Audio that hasn't been streamed to the engine yet. Marks it as streamed.
    pub fn take_unstreamed(&mut self) -> &[f32] {
        let start = self.streamed;
        self.streamed = self.samples.len();
        &self.samples[start..]
    }

    /// Marks everything as not streamed yet, for a new engine session.
    pub fn rewind(&mut self) {
        self.streamed = 0;
    }

    /// Audio waiting to be streamed, in samples.
    pub fn unstreamed_len(&self) -> usize {
        self.samples.len() - self.streamed
    }

    /// True when the microphone delivered nothing but silence, which almost
    /// always means it is muted or the wrong device is selected.
    pub fn sounds_muted(&self) -> bool {
        self.peak < MUTED_PEAK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_hands_over_each_sample_once() {
        let mut r = Recording::new();
        r.push(&[0.1; 100]);
        assert_eq!(r.take_unstreamed().len(), 100);
        assert!(r.take_unstreamed().is_empty());
        r.push(&[0.1; 30]);
        assert_eq!(r.take_unstreamed().len(), 30);
        assert_eq!(r.samples().len(), 130);
    }

    #[test]
    fn warns_once_then_stops_at_the_limit() {
        let mut r = Recording::new();
        let minute = vec![0.0; SAMPLE_RATE as usize * 60];
        for _ in 0..18 {
            assert_eq!(r.push(&minute), Limit::None);
        }
        assert_eq!(r.push(&minute), Limit::Warn);
        assert_eq!(r.push(&minute[..SAMPLE_RATE as usize]), Limit::None);
        assert_eq!(r.push(&minute), Limit::Reached);
        assert_eq!(r.duration_ms(), MAX_RECORDING_MS);
        assert_eq!(r.push(&minute), Limit::Reached);
        assert_eq!(r.duration_ms(), MAX_RECORDING_MS);
    }

    #[test]
    fn detects_a_muted_microphone() {
        let mut r = Recording::new();
        r.push(&[0.0005, -0.001, 0.0]);
        assert!(r.sounds_muted());
        r.push(&[0.2]);
        assert!(!r.sounds_muted());
    }
}
