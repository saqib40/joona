//! Microphone capture, resampling to 16 kHz mono, and level metering.

mod capture;
pub mod level;
mod privacy;
mod recording;
pub mod resample;
mod test;

pub use capture::{AudioSink, Capture, CaptureInfo, InputDevice, list_input_devices};
pub use privacy::{MicAccess, PRIVACY_SETTINGS_URL, mic_access};
pub use recording::{Limit, MAX_RECORDING_MS, Recording, WARN_RECORDING_MS};
pub use resample::TARGET_RATE;
pub use test::MicTest;

/// Samples per second of the audio handed to engines and providers.
pub const SAMPLE_RATE: u32 = TARGET_RATE;

pub fn samples_to_ms(samples: usize) -> u64 {
    samples as u64 * 1000 / SAMPLE_RATE as u64
}

/// Encodes 16 kHz mono samples as a 16-bit PCM WAV file in memory, for cloud
/// providers that want a file upload.
pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend(encode_pcm16(samples));
    out
}

/// The samples as raw 16-bit little-endian PCM, without a header.
pub fn encode_pcm16(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|&s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16).to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_well_formed() {
        let wav = encode_wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(wav.len(), 44 + 6);
    }
}
