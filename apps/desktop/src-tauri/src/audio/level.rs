//! Microphone level for the bar's moving bars, and loudness normalisation.

/// Turns audio into a smoothed 0..1 level, reported at a fixed rate.
pub struct LevelMeter {
    window: usize,
    acc: f64,
    n: usize,
    smoothed: f32,
}

impl LevelMeter {
    /// `window` is the number of 16 kHz samples per reported level.
    pub fn new(window: usize) -> Self {
        Self {
            window,
            acc: 0.0,
            n: 0,
            smoothed: 0.0,
        }
    }

    /// Feeds samples and calls `report` once per full window.
    pub fn push(&mut self, samples: &[f32], mut report: impl FnMut(f32)) {
        for &s in samples {
            self.acc += (s as f64) * (s as f64);
            self.n += 1;
            if self.n == self.window {
                let rms = (self.acc / self.n as f64).sqrt() as f32;
                let level = rms_to_level(rms);
                // Fast attack, slower release, so speech looks lively without flicker.
                let k = if level > self.smoothed { 0.6 } else { 0.25 };
                self.smoothed += (level - self.smoothed) * k;
                report(self.smoothed);
                self.acc = 0.0;
                self.n = 0;
            }
        }
    }
}

/// Maps RMS to 0..1 on a decibel scale from -60 dBFS (silent) to -12 dBFS (loud).
pub fn rms_to_level(rms: f32) -> f32 {
    if rms <= 1e-6 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db + 60.0) / 48.0).clamp(0.0, 1.0)
}

/// Raises quiet or whispered recordings to a usable level. Gain is capped so
/// background noise in a silent clip isn't blown up into fake speech.
pub fn normalize(samples: &mut [f32]) {
    const TARGET_PEAK: f32 = 0.9;
    const MAX_GAIN: f32 = 8.0;
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= 1e-4 {
        return;
    }
    let gain = (TARGET_PEAK / peak).min(MAX_GAIN);
    if gain <= 1.05 {
        return;
    }
    for s in samples.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_scale() {
        assert_eq!(rms_to_level(0.0), 0.0);
        assert!(rms_to_level(0.001) < 0.01);
        assert_eq!(rms_to_level(0.3), 1.0);
        let mid = rms_to_level(0.01);
        assert!(mid > 0.3 && mid < 0.5, "{mid}");
    }

    #[test]
    fn meter_reports_once_per_window() {
        let mut m = LevelMeter::new(160);
        let mut reports = Vec::new();
        m.push(&vec![0.2; 500], |l| reports.push(l));
        assert_eq!(reports.len(), 3);
        assert!(reports.windows(2).all(|w| w[1] >= w[0]), "attack rises");
    }

    #[test]
    fn whisper_gets_louder_but_not_beyond_the_cap() {
        let mut quiet = vec![0.05, -0.05, 0.02];
        normalize(&mut quiet);
        assert!((quiet[0] - 0.4).abs() < 1e-6, "capped at 8x: {}", quiet[0]);

        let mut medium = vec![0.3, -0.2];
        normalize(&mut medium);
        assert!((medium[0] - 0.9).abs() < 1e-6);

        let mut loud = vec![0.95, -0.5];
        normalize(&mut loud);
        assert_eq!(loud, vec![0.95, -0.5]);

        let mut silent = vec![0.0; 10];
        normalize(&mut silent);
        assert!(silent.iter().all(|s| *s == 0.0));
    }
}
