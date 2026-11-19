//! Transcribes a WAV file through the supervised engine, the way the app does,
//! then kills the engine and transcribes again to check crash recovery.
//!
//! cargo run --example engine_probe -- <16 kHz mono wav> <installed model id or ggml file> [en,hi]

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, bail};
use joona_lib::supervisor::{EngineEvent, Supervisor, Variant, finish_timeout};
use joona_protocol::{ConfigureParams, EngineKind, ModelSpec, OpenParams, Task, samples_to_ms};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "joona_lib=debug,joona::engine=info".into()),
        )
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .init();

    let mut args = std::env::args().skip(1);
    let wav = PathBuf::from(
        args.next()
            .context("usage: engine_probe <wav> <model> [candidates]")?,
    );
    let model = PathBuf::from(args.next().context("missing the model path")?);
    let candidates: Vec<String> = args
        .next()
        .map(|c| c.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    let audio = read_wav(&wav)?;

    let engine = Supervisor::new(Variant::Cpu);
    let mut events = engine.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                EngineEvent::Started { variant, info } => {
                    println!(
                        "event: started {variant:?} engine {} (gpus: {})",
                        info.version,
                        info.gpus.len()
                    )
                }
                EngineEvent::Crashed {
                    message,
                    restart_in,
                } => {
                    println!("event: crashed ({message}), restarting in {restart_in:?}")
                }
                EngineEvent::FellBackToCpu { reason } => {
                    println!("event: fell back to CPU ({reason})")
                }
            }
        }
    });

    let vad = joona_lib::paths::models_dir()
        .join("silero-vad")
        .join("silero_vad_v4.onnx");
    engine
        .configure(ConfigureParams {
            vad_model: vad.is_file().then_some(vad),
            ..Default::default()
        })
        .await?;
    let library = joona_lib::models::Library::new(joona_lib::paths::models_dir());
    let spec = match library.get(&model.to_string_lossy()) {
        Some(installed) => installed.spec(),
        None => ModelSpec {
            id: "whisper-probe".into(),
            engine: EngineKind::Whisper,
            path: model,
            variant: None,
        },
    };
    let id = spec.id.clone();
    let loaded = engine.load(spec, false).await?;
    println!(
        "loaded {} on {:?} in {} ms",
        loaded.model, loaded.device, loaded.load_ms
    );

    transcribe(&engine, &id, &audio, &candidates).await?;

    println!("killing the engine to test recovery");
    engine.crash_for_test().await;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    transcribe(&engine, &id, &audio, &candidates).await?;

    let status = engine.status().await?;
    println!(
        "status: {} model(s) loaded, busy {}",
        status.loaded.len(),
        status.busy
    );
    let freed = engine.unload_idle(std::time::Duration::ZERO).await?;
    println!("unloaded idle: {freed:?}");
    engine.stop().await;
    println!("stopped");
    Ok(())
}

async fn transcribe(
    engine: &Supervisor,
    model: &str,
    audio: &[f32],
    candidates: &[String],
) -> anyhow::Result<()> {
    let session = engine
        .open(OpenParams {
            model: model.into(),
            task: Task::Transcribe,
            language: None,
            candidates: candidates.to_vec(),
            prompt: None,
            vad: true,
        })
        .await?;
    for chunk in audio.chunks(3200) {
        session.feed(chunk)?;
    }
    let started = Instant::now();
    let result = session
        .finish(finish_timeout(samples_to_ms(audio.len())))
        .await?;
    println!(
        "[{} ms] {:?} (language {:?}, p {:?}, speech {} of {} ms, inference {} ms on {:?})",
        started.elapsed().as_millis(),
        result.text,
        result.language,
        result.language_probability,
        result.speech_ms,
        result.audio_ms,
        result.infer_ms,
        result.device
    );
    Ok(())
}

/// Reads a 16 kHz mono PCM16 WAV file.
fn read_wav(path: &Path) -> anyhow::Result<Vec<f32>> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a WAV file");
    }
    let mut pos = 12;
    let mut format = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into()?) as usize;
        let body = &bytes[pos + 8..(pos + 8 + len).min(bytes.len())];
        match id {
            b"fmt " => {
                let channels = u16::from_le_bytes(body[2..4].try_into()?);
                let rate = u32::from_le_bytes(body[4..8].try_into()?);
                let bits = u16::from_le_bytes(body[14..16].try_into()?);
                format = Some((channels, rate, bits));
            }
            b"data" => {
                if format != Some((1, 16_000, 16)) {
                    bail!("expected 16 kHz mono 16-bit audio, found {format:?}");
                }
                return Ok(body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&p| i16::from_le_bytes(p) as f32 / i16::MAX as f32)
                    .collect());
            }
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    bail!("no audio data in the file")
}
