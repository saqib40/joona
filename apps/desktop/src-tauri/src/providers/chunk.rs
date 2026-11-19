//! Splits long recordings for services that take a limited length per request.

use std::ops::Range;

/// 20 ms at 16 kHz.
const FRAME: usize = 320;
/// Loudness is judged over this many frames (300 ms), so a quiet moment
/// inside a word doesn't pass for a pause.
const WINDOW: usize = 15;

/// Splits `samples` into consecutive pieces of at most `max` samples. Each
/// cut falls in the quietest moment of the last 60% of its piece, which is
/// almost always a pause between words.
pub fn split_at_pauses(samples: &[f32], max: usize) -> Vec<Range<usize>> {
    if samples.len() <= max || max < FRAME * 2 {
        return std::iter::once(0..samples.len()).collect();
    }
    // Energy per frame as a running total, for quick averages over a window.
    let mut total = Vec::with_capacity(samples.len() / FRAME + 2);
    total.push(0.0f64);
    for frame in samples.chunks(FRAME) {
        let energy = frame.iter().map(|&s| f64::from(s * s)).sum::<f64>() / frame.len() as f64;
        total.push(total[total.len() - 1] + energy);
    }
    let frames = total.len() - 1;
    // How loud it is around the start of frame `i`.
    let around = |i: usize| {
        let lo = i.saturating_sub(WINDOW / 2);
        let hi = (i + WINDOW / 2 + 1).min(frames);
        (total[hi] - total[lo]) / (hi - lo) as f64
    };

    let mut pieces = Vec::new();
    let mut start = 0;
    while samples.len() - start > max {
        let earliest = (start + max * 2 / 5).div_ceil(FRAME);
        let latest = (start + max) / FRAME;
        // From the latest moment back, moving only for a clearly quieter one,
        // so pieces stay long and there are fewer requests.
        let mut cut = latest;
        let mut quietest = around(latest);
        for frame in (earliest..latest).rev() {
            let loudness = around(frame);
            if loudness < quietest * 0.9 {
                cut = frame;
                quietest = loudness;
            }
        }
        pieces.push(start..cut * FRAME);
        start = cut * FRAME;
    }
    pieces.push(start..samples.len());
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: usize = 16_000;
    const MAX: usize = 25 * RATE;

    fn speech(seconds: f32) -> Vec<f32> {
        let n = (seconds * RATE as f32) as usize;
        (0..n)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 200.0 / RATE as f32).sin() * 0.3)
            .collect()
    }

    fn pause(seconds: f32) -> Vec<f32> {
        vec![0.001; (seconds * RATE as f32) as usize]
    }

    fn assert_covers(pieces: &[Range<usize>], len: usize) {
        assert_eq!(pieces.first().map(|p| p.start), Some(0));
        assert_eq!(pieces.last().map(|p| p.end), Some(len));
        assert!(pieces.windows(2).all(|w| w[0].end == w[1].start));
        assert!(pieces.iter().all(|p| p.len() <= MAX), "{pieces:?}");
    }

    #[test]
    fn short_recordings_stay_whole() {
        for audio in [speech(24.0), Vec::new()] {
            let pieces = split_at_pauses(&audio, MAX);
            assert_eq!(pieces.len(), 1);
            assert_covers(&pieces, audio.len());
        }
    }

    #[test]
    fn cuts_fall_in_pauses() {
        let audio = [
            speech(18.0),
            pause(0.6),
            speech(17.0),
            pause(0.8),
            speech(12.0),
        ]
        .concat();
        let pieces = split_at_pauses(&audio, MAX);
        assert_covers(&pieces, audio.len());
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        let first = 18 * RATE..18 * RATE + RATE * 6 / 10;
        assert!(first.contains(&pieces[0].end), "{pieces:?}");
        let second = 35 * RATE + RATE * 6 / 10..36 * RATE + RATE * 4 / 10;
        assert!(second.contains(&pieces[1].end), "{pieces:?}");
    }

    #[test]
    fn speech_without_pauses_is_still_cut_to_size() {
        let audio = speech(70.0);
        let pieces = split_at_pauses(&audio, MAX);
        assert_covers(&pieces, audio.len());
        assert_eq!(pieces.len(), 3);
        assert!(
            pieces[..2].iter().all(|p| p.len() > MAX - FRAME),
            "{pieces:?}"
        );
    }

    #[test]
    fn an_early_pause_doesnt_make_a_tiny_piece() {
        let audio = [speech(3.0), pause(1.0), speech(40.0)].concat();
        let pieces = split_at_pauses(&audio, MAX);
        assert_covers(&pieces, audio.len());
        assert!(pieces[0].len() >= MAX * 2 / 5, "{pieces:?}");
    }
}
