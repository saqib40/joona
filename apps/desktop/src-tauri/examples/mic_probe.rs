//! Lists microphones, records a few seconds and saves a 16 kHz WAV.
//! `cargo run -p joona --example mic_probe -- 3 out.wav [device-id]`

use std::sync::{Arc, Mutex};
use std::time::Duration;

use joona_lib::audio::{AudioSink, Capture, Recording, encode_wav, list_input_devices};

struct Sink {
    recording: Arc<Mutex<Recording>>,
    levels: Arc<Mutex<Vec<f32>>>,
}

impl AudioSink for Sink {
    fn on_audio(&mut self, samples: &[f32]) {
        self.recording.lock().unwrap().push(samples);
    }
    fn on_level(&mut self, level: f32) {
        self.levels.lock().unwrap().push(level);
    }
    fn on_error(&mut self, message: String) {
        eprintln!("capture error: {message}");
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let out = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("joona-mic.wav")
            .display()
            .to_string()
    });
    let device = args.next();

    for d in list_input_devices() {
        println!(
            "{} {}  [{}]",
            if d.is_default { "*" } else { " " },
            d.name,
            d.id
        );
    }

    let recording = Arc::new(Mutex::new(Recording::new()));
    let levels = Arc::new(Mutex::new(Vec::new()));
    let capture = Capture::start(
        device.as_deref(),
        Box::new(Sink {
            recording: recording.clone(),
            levels: levels.clone(),
        }),
    )?;
    println!(
        "recording {secs}s from {} ({} Hz, {} ch)",
        capture.info.device_name, capture.info.sample_rate, capture.info.channels
    );
    std::thread::sleep(Duration::from_secs(secs));
    capture.stop();

    let recording = recording.lock().unwrap();
    let levels = levels.lock().unwrap();
    let peak = levels.iter().fold(0.0f32, |m, l| m.max(*l));
    println!(
        "{} ms of audio, {} level reports, peak level {peak:.2}, muted: {}",
        recording.duration_ms(),
        levels.len(),
        recording.sounds_muted()
    );
    std::fs::write(&out, encode_wav(recording.samples()))?;
    println!("saved {out}");
    Ok(())
}
