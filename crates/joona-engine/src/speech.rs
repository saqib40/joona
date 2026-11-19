//! Finds the speech in a recording so models never see long silences, which is
//! where Whisper invents text, and evens out quiet microphones.

use std::ops::Range;
use std::path::Path;

use joona_protocol::{ms_to_samples, samples_to_ms};
use transcribe_rs::vad::{SileroVad, Vad as _};

/// Silero v4 scores 30 ms frames of 16 kHz audio.
pub const FRAME: usize = 480;

const START_PROB: f32 = 0.35;
const END_PROB: f32 = 0.2;
/// Shorter bursts are clicks, breaths or key presses.
const MIN_SPEECH_FRAMES: usize = 4;
/// A pause this long (360 ms) ends a stretch of speech.
const MIN_SILENCE_FRAMES: usize = 12;
/// Kept around each stretch so soft word onsets and endings survive.
const PAD_MS: u64 = 200;

pub struct Detector {
    vad: SileroVad,
}

impl Detector {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let vad = SileroVad::new(path, START_PROB).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { vad })
    }

    pub fn reset(&mut self) {
        self.vad.reset();
    }

    /// Speech probability of one [`FRAME`]. Errors count as speech, so a VAD
    /// problem can only cost speed, never words.
    pub fn score(&mut self, frame: &[f32]) -> f32 {
        self.vad.speech_probability(frame).unwrap_or(1.0)
    }
}

/// Per-session frame scores, filled in while audio streams in.
#[derive(Default)]
pub struct Scores {
    pub probs: Vec<f32>,
    scored: usize,
}

impl Scores {
    /// Scores every complete frame of `audio` not scored yet.
    pub fn update(&mut self, detector: &mut Detector, audio: &[f32]) {
        while self.scored + FRAME <= audio.len() {
            self.probs
                .push(detector.score(&audio[self.scored..self.scored + FRAME]));
            self.scored += FRAME;
        }
    }

    /// Scores the final partial frame, padded with silence.
    pub fn finish(&mut self, detector: &mut Detector, audio: &[f32]) {
        self.update(detector, audio);
        if self.scored < audio.len() {
            let mut last = audio[self.scored..].to_vec();
            last.resize(FRAME, 0.0);
            self.probs.push(detector.score(&last));
            self.scored = audio.len();
        }
    }
}

/// Sample ranges that contain speech, padded and merged.
pub fn speech_regions(probs: &[f32], len: usize) -> Vec<Range<usize>> {
    let mut frames: Vec<Range<usize>> = Vec::new();
    let mut start: Option<usize> = None;
    let mut silence = 0;
    for (i, &p) in probs.iter().enumerate() {
        match start {
            None => {
                if p >= START_PROB {
                    start = Some(i);
                    silence = 0;
                }
            }
            Some(s) => {
                if p < END_PROB {
                    silence += 1;
                    if silence >= MIN_SILENCE_FRAMES {
                        let end = i + 1 - silence;
                        if end - s >= MIN_SPEECH_FRAMES {
                            frames.push(s..end);
                        }
                        start = None;
                    }
                } else {
                    silence = 0;
                }
            }
        }
    }
    if let Some(s) = start {
        let end = probs.len() - silence;
        if end > s && end - s >= MIN_SPEECH_FRAMES {
            frames.push(s..end);
        }
    }

    let pad = ms_to_samples(PAD_MS);
    let mut out: Vec<Range<usize>> = Vec::new();
    for f in frames {
        let a = (f.start * FRAME).saturating_sub(pad).min(len);
        let b = (f.end * FRAME + pad).min(len);
        if a >= b {
            continue;
        }
        match out.last_mut() {
            Some(last) if a <= last.end => last.end = last.end.max(b),
            _ => out.push(a..b),
        }
    }
    out
}

pub struct Prepared {
    pub audio: Vec<f32>,
    pub speech_ms: u64,
    /// Offsets in `audio` where one stretch of speech ends and the next begins.
    /// These are the safest places to split long audio.
    pub breaks: Vec<usize>,
}

/// Keeps only the speech (when frame scores are available) and normalises it.
pub fn prepare(audio: &[f32], probs: Option<&[f32]>) -> Prepared {
    let mut breaks = Vec::new();
    let mut out = match probs {
        Some(p) => {
            let mut kept = Vec::with_capacity(audio.len());
            for r in speech_regions(p, audio.len()) {
                if !kept.is_empty() {
                    breaks.push(kept.len());
                }
                kept.extend_from_slice(&audio[r]);
            }
            kept
        }
        None => audio.to_vec(),
    };
    let speech_ms = samples_to_ms(out.len());
    normalize(&mut out);
    Prepared {
        audio: out,
        speech_ms,
        breaks,
    }
}

/// Splits audio into pieces no longer than `max` samples, for models that
/// only handle short clips. Cuts go at pauses between stretches of speech when
/// possible, otherwise at the quietest moment near the limit.
pub fn chunks(audio: &[f32], breaks: &[usize], max: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    while audio.len() - start > max {
        let limit = start + max;
        let cut = breaks
            .iter()
            .rev()
            .copied()
            .find(|&b| b > start + max / 3 && b <= limit)
            .unwrap_or_else(|| {
                quietest_point(audio, limit - (max / 4).min(ms_to_samples(3000)), limit)
            });
        out.push(start..cut);
        start = cut;
    }
    if start < audio.len() {
        out.push(start..audio.len());
    }
    out
}

fn quietest_point(audio: &[f32], from: usize, to: usize) -> usize {
    let mut best = to;
    let mut best_energy = f32::MAX;
    let mut i = from;
    while i + FRAME <= to {
        let energy: f32 = audio[i..i + FRAME].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best = i + FRAME / 2;
        }
        i += FRAME;
    }
    best
}

/// Raises quiet or whispered speech to a usable level. Gain is capped so the
/// hiss of a near-silent clip isn't blown up into something that looks like speech.
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

    fn probs(pattern: &[(f32, usize)]) -> Vec<f32> {
        pattern
            .iter()
            .flat_map(|&(p, n)| std::iter::repeat_n(p, n))
            .collect()
    }

    #[test]
    fn silence_has_no_speech() {
        let p = probs(&[(0.05, 100)]);
        assert!(speech_regions(&p, 100 * FRAME).is_empty());
    }

    #[test]
    fn one_utterance_is_found_and_padded() {
        let p = probs(&[(0.0, 50), (0.9, 40), (0.0, 50)]);
        let r = speech_regions(&p, 140 * FRAME);
        let pad = ms_to_samples(PAD_MS);
        assert_eq!(r, vec![50 * FRAME - pad..90 * FRAME + pad]);
    }

    #[test]
    fn short_pauses_stay_inside_one_region() {
        let p = probs(&[(0.9, 30), (0.0, 8), (0.9, 30)]);
        assert_eq!(speech_regions(&p, 68 * FRAME).len(), 1);
    }

    #[test]
    fn long_pauses_split_and_nearby_regions_merge() {
        let p = probs(&[(0.9, 30), (0.0, 100), (0.9, 30)]);
        assert_eq!(speech_regions(&p, 160 * FRAME).len(), 2);
        // This pause is just long enough to end speech, but the padding bridges it.
        let p = probs(&[(0.9, 30), (0.0, 13), (0.9, 30)]);
        assert_eq!(speech_regions(&p, 73 * FRAME).len(), 1);
    }

    #[test]
    fn clicks_are_ignored() {
        let p = probs(&[(0.0, 20), (0.9, 2), (0.0, 20)]);
        assert!(speech_regions(&p, 42 * FRAME).is_empty());
    }

    #[test]
    fn speech_running_to_the_end_is_kept() {
        let p = probs(&[(0.0, 20), (0.9, 20)]);
        let r = speech_regions(&p, 40 * FRAME);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].end, 40 * FRAME);
    }

    #[test]
    fn hysteresis_keeps_soft_speech() {
        // Dips between the end and start thresholds don't count as silence.
        let p = probs(&[(0.8, 10), (0.25, 30), (0.8, 10)]);
        assert_eq!(speech_regions(&p, 50 * FRAME).len(), 1);
    }

    #[test]
    fn prepare_keeps_only_speech() {
        let audio = vec![0.1f32; 140 * FRAME];
        let p = probs(&[(0.0, 50), (0.9, 40), (0.0, 50)]);
        let out = prepare(&audio, Some(&p));
        let expected = 40 * FRAME + 2 * ms_to_samples(PAD_MS);
        assert_eq!(out.audio.len(), expected);
        assert_eq!(out.speech_ms, samples_to_ms(expected));
        assert!(
            (out.audio[0] - 0.8).abs() < 1e-6,
            "normalised with the 8x cap"
        );
        let untouched = prepare(&audio, None);
        assert_eq!(untouched.audio.len(), audio.len());
    }

    #[test]
    fn prepare_records_where_regions_join() {
        let audio = vec![0.1f32; 200 * FRAME];
        let p = probs(&[(0.9, 30), (0.0, 100), (0.9, 30), (0.0, 40)]);
        let out = prepare(&audio, Some(&p));
        assert_eq!(out.breaks, vec![30 * FRAME + ms_to_samples(PAD_MS)]);
    }

    #[test]
    fn chunks_prefer_pauses_and_cover_everything() {
        let audio = vec![0.1f32; 100];
        assert_eq!(chunks(&audio, &[], 200), vec![0..100]);
        assert_eq!(chunks(&audio, &[40, 70], 60), vec![0..40, 40..100]);

        let mut audio = vec![0.5f32; 20 * FRAME];
        audio[14 * FRAME..17 * FRAME].fill(0.0);
        let c = chunks(&audio, &[], 18 * FRAME);
        assert_eq!(c.len(), 2);
        assert!(
            (14 * FRAME..17 * FRAME).contains(&c[0].end),
            "cut in the quiet part: {:?}",
            c[0]
        );
        assert_eq!(c[0].end, c[1].start);
        assert_eq!(c[1].end, audio.len());
    }
}
