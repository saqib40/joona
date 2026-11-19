//! Short cues for starting, stopping and cancelling a dictation.
//!
//! The cues are synthesised once at startup, so there are no sound files to
//! ship or license, and played asynchronously so they never delay a dictation.

use std::f32::consts::TAU;
use std::sync::LazyLock;

use crate::audio::{SAMPLE_RATE, encode_wav};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Cancel,
    Error,
}

/// A note: frequency in Hz, start and length in milliseconds.
type Note = (f32, u32, u32);

const VOLUME: f32 = 0.16;

fn notes(cue: Cue) -> &'static [Note] {
    match cue {
        // E5 then A5: rising, "go".
        Cue::Start => &[(659.25, 0, 70), (880.0, 55, 110)],
        // A5 then E5: falling, "done".
        Cue::Stop => &[(880.0, 0, 70), (659.25, 55, 110)],
        // C5 then G4: a softer fall.
        Cue::Cancel => &[(523.25, 0, 70), (392.0, 60, 120)],
        Cue::Error => &[(329.63, 0, 90), (329.63, 130, 110)],
    }
}

fn synthesize(notes: &[Note]) -> Vec<u8> {
    let end_ms = notes
        .iter()
        .map(|(_, start, len)| start + len)
        .max()
        .unwrap_or(0)
        + 20;
    let rate = SAMPLE_RATE as f32;
    let mut samples = vec![0.0f32; (end_ms * SAMPLE_RATE / 1000) as usize];
    for &(freq, start, len) in notes {
        let first = (start * SAMPLE_RATE / 1000) as usize;
        let count = (len * SAMPLE_RATE / 1000) as usize;
        for i in 0..count {
            let t = i as f32 / rate;
            let progress = i as f32 / count as f32;
            // 5 ms attack, then an exponential decay: a soft "tink".
            let attack = (t / 0.005).min(1.0);
            let envelope = attack * (-4.0 * progress).exp() * (1.0 - progress);
            // A quiet octave above gives the tone a little shimmer.
            let tone = (TAU * freq * t).sin() + 0.25 * (TAU * 2.0 * freq * t).sin();
            if let Some(s) = samples.get_mut(first + i) {
                *s += VOLUME * envelope * tone / 1.25;
            }
        }
    }
    encode_wav(&samples)
}

static START: LazyLock<Vec<u8>> = LazyLock::new(|| synthesize(notes(Cue::Start)));
static STOP: LazyLock<Vec<u8>> = LazyLock::new(|| synthesize(notes(Cue::Stop)));
static CANCEL: LazyLock<Vec<u8>> = LazyLock::new(|| synthesize(notes(Cue::Cancel)));
static ERROR: LazyLock<Vec<u8>> = LazyLock::new(|| synthesize(notes(Cue::Error)));

fn wav(cue: Cue) -> &'static [u8] {
    match cue {
        Cue::Start => &START,
        Cue::Stop => &STOP,
        Cue::Cancel => &CANCEL,
        Cue::Error => &ERROR,
    }
}

/// Builds the cues ahead of time, so the first dictation doesn't wait.
pub fn prepare() {
    for cue in [Cue::Start, Cue::Stop, Cue::Cancel, Cue::Error] {
        let _ = wav(cue);
    }
}

#[cfg(windows)]
pub fn play(cue: Cue) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    use windows::core::PCWSTR;

    let data = wav(cue);
    // With SND_MEMORY the "name" is a pointer to the WAV bytes, which are
    // static, so they outlive the asynchronous playback.
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(data.as_ptr().cast()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(target_os = "macos")]
pub fn play(cue: Cue) {
    use objc2::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;

    let data = wav(cue);
    let ns_data = NSData::with_bytes(data);
    if let Some(sound) = NSSound::initWithData(NSSound::alloc(), &ns_data) {
        sound.play();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn play(_cue: Cue) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_short_and_quiet() {
        for cue in [Cue::Start, Cue::Stop, Cue::Cancel, Cue::Error] {
            let wav = wav(cue);
            let samples = (wav.len() - 44) / 2;
            assert!(
                samples < SAMPLE_RATE as usize / 2,
                "{cue:?} is under half a second"
            );
            let peak = wav[44..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&b| i16::from_le_bytes(b).unsigned_abs())
                .max()
                .unwrap();
            assert!(
                peak > 1000 && peak < (i16::MAX as f32 * 0.3) as u16,
                "{cue:?} peak {peak}"
            );
        }
    }
}
