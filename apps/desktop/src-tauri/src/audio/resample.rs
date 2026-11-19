//! Streaming conversion of mono audio at the device rate to 16 kHz.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

pub const TARGET_RATE: u32 = 16_000;
const CHUNK: usize = 1024;

pub struct StreamResampler {
    inner: Option<Inner>,
}

struct Inner {
    fft: Fft<f32>,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    /// Leading output frames that are only the resampler's start-up delay.
    to_trim: usize,
    produced: usize,
    consumed: usize,
    ratio: f64,
}

impl StreamResampler {
    pub fn new(input_rate: u32) -> anyhow::Result<Self> {
        if input_rate == TARGET_RATE {
            return Ok(Self { inner: None });
        }
        let fft = Fft::<f32>::new(
            input_rate as usize,
            TARGET_RATE as usize,
            CHUNK,
            1,
            FixedSync::Input,
        )?;
        let to_trim = fft.output_delay();
        let scratch = vec![0.0; fft.output_frames_max()];
        Ok(Self {
            inner: Some(Inner {
                fft,
                pending: Vec::with_capacity(CHUNK * 2),
                scratch,
                to_trim,
                produced: 0,
                consumed: 0,
                ratio: TARGET_RATE as f64 / input_rate as f64,
            }),
        })
    }

    /// Resamples `input` and appends whatever output is ready to `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let Some(inner) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return;
        };
        inner.pending.extend_from_slice(input);
        while inner.pending.len() >= inner.fft.input_frames_next() {
            let need = inner.fft.input_frames_next();
            inner.step(need, None, usize::MAX, out);
            inner.pending.drain(..need);
        }
    }

    /// Flushes buffered audio so the output covers all input pushed so far.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let Some(inner) = self.inner.as_mut() else {
            return;
        };
        let expected =
            ((inner.consumed + inner.pending.len()) as f64 * inner.ratio).round() as usize;
        // The first step carries the leftover input; later ones pump silence
        // until the resampler's delayed tail has come out.
        let mut partial = inner.pending.len();
        for _ in 0..64 {
            if inner.produced >= expected {
                break;
            }
            let need = inner.fft.input_frames_next();
            inner.pending.resize(need, 0.0);
            inner.step(need, Some(partial.min(need)), expected, out);
            inner.pending.clear();
            partial = 0;
        }
        inner.pending.clear();
    }
}

impl Inner {
    fn step(&mut self, need: usize, partial: Option<usize>, limit: usize, out: &mut Vec<f32>) {
        let input =
            InterleavedSlice::new(&self.pending[..need], 1, need).expect("valid input buffer");
        let frames = self.scratch.len();
        let mut output =
            InterleavedSlice::new_mut(&mut self.scratch, 1, frames).expect("valid output buffer");
        let indexing = partial.map(|p| Indexing::new().partial_len(p));
        let written = match self
            .fft
            .process_into_buffer(&input, &mut output, indexing.as_ref())
        {
            Ok((_, written)) => written,
            Err(e) => {
                tracing::warn!("resampler error: {e}");
                0
            }
        };
        self.consumed += partial.unwrap_or(need);
        self.emit(written, limit, out);
    }

    fn emit(&mut self, written: usize, limit: usize, out: &mut Vec<f32>) {
        let mut start = 0;
        if self.to_trim > 0 {
            let trim = self.to_trim.min(written);
            self.to_trim -= trim;
            start = trim;
        }
        let available = written - start;
        let room = limit.saturating_sub(self.produced);
        let take = available.min(room);
        out.extend_from_slice(&self.scratch[start..start + take]);
        self.produced += take;
    }
}

/// Averages interleaved channels into mono.
pub fn downmix_into(interleaved: impl Iterator<Item = f32>, channels: usize, out: &mut Vec<f32>) {
    if channels <= 1 {
        out.extend(interleaved);
        return;
    }
    let scale = 1.0 / channels as f32;
    let mut acc = 0.0;
    let mut n = 0;
    for s in interleaved {
        acc += s;
        n += 1;
        if n == channels {
            out.push(acc * scale);
            acc = 0.0;
            n = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.5)
            .collect()
    }

    fn resample_in_pieces(rate: u32, input: &[f32], piece: usize) -> Vec<f32> {
        let mut r = StreamResampler::new(rate).unwrap();
        let mut out = Vec::new();
        for chunk in input.chunks(piece) {
            r.push(chunk, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn output_length_matches_the_rate_ratio() {
        for rate in [48_000, 44_100, 32_000, 22_050, 96_000] {
            let input = sine(rate, 440.0, 1.3);
            let out = resample_in_pieces(rate, &input, 480);
            let expected = (input.len() as f64 * TARGET_RATE as f64 / rate as f64).round() as i64;
            assert!(
                (out.len() as i64 - expected).abs() <= 2,
                "{rate} Hz: got {} samples, expected about {expected}",
                out.len()
            );
        }
    }

    #[test]
    fn a_tone_keeps_its_level_and_frequency() {
        let out = resample_in_pieces(48_000, &sine(48_000, 1_000.0, 1.0), 441);
        // Skip the edges, then check amplitude and zero crossings.
        let body = &out[1_600..out.len() - 1_600];
        let peak = body.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.05, "peak {peak}");
        let crossings = body
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        let secs = body.len() as f32 / TARGET_RATE as f32;
        let freq = crossings as f32 / secs;
        assert!((freq - 1_000.0).abs() < 15.0, "frequency {freq}");
    }

    #[test]
    fn sixteen_khz_input_passes_through() {
        let input = sine(16_000, 300.0, 0.2);
        let out = resample_in_pieces(16_000, &input, 100);
        assert_eq!(out, input);
    }

    #[test]
    fn short_clips_survive() {
        let input = sine(48_000, 440.0, 0.05);
        let out = resample_in_pieces(48_000, &input, 4096);
        assert!((out.len() as i64 - 800).abs() <= 2, "got {}", out.len());
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_into([1.0, 0.0, 0.5, 0.5, -1.0, 1.0].into_iter(), 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }
}
