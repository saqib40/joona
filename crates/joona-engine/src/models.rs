//! The speech models the engine can hold in memory.

use std::path::Path;
use std::time::Instant;

use joona_protocol::{Device, EngineKind, ModelSpec, Segment, Task, ms_to_samples};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineParams, MoonshineVariant};
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams, TimestampGranularity};

use crate::speech;
use crate::whisper::{self, Whisper};

/// Parakeet handles long input, but memory grows quickly past this.
const PARAKEET_CHUNK_MS: u64 = 30_000;
/// Moonshine is trained on short utterances and degrades on long ones.
const MOONSHINE_CHUNK_MS: u64 = 20_000;

enum Kind {
    Whisper(Box<Whisper>),
    Parakeet(Box<ParakeetModel>),
    Moonshine(Box<MoonshineModel>),
}

pub struct Model {
    pub spec: ModelSpec,
    kind: Kind,
    /// Whether the GPU was asked for, so a changed setting triggers a reload.
    pub wanted_gpu: bool,
    pub device: Device,
    pub gpu_name: Option<String>,
    pub last_used: Instant,
}

pub struct Request<'a> {
    pub task: Task,
    pub language: Option<&'a str>,
    pub candidates: &'a [String],
    pub prompt: Option<&'a str>,
    pub threads: i32,
    pub beam_size: i32,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub struct Output {
    pub text: String,
    pub language: Option<String>,
    pub language_probability: Option<f32>,
    pub segments: Vec<Segment>,
}

impl Model {
    pub fn load(spec: ModelSpec, use_gpu: bool, threads: i32) -> anyhow::Result<Self> {
        if !spec.path.exists() {
            anyhow::bail!("model files not found at {}", spec.path.display());
        }
        let quantization = quantization(spec.variant.as_deref());
        let (kind, device, gpu_name) = match spec.engine {
            EngineKind::Whisper => {
                let mut w = Whisper::load(&spec.path, use_gpu)?;
                w.warm_up(threads);
                let (device, gpu) = (w.device, w.gpu_name.clone());
                (Kind::Whisper(Box::new(w)), device, gpu)
            }
            EngineKind::Parakeet => {
                let mut m = ParakeetModel::load(&spec.path, &quantization)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let _ =
                    m.transcribe_with(&vec![0.0; ms_to_samples(1000)], &ParakeetParams::default());
                (Kind::Parakeet(Box::new(m)), Device::Cpu, None)
            }
            EngineKind::Moonshine => {
                let variant = moonshine_variant(spec.variant.as_deref(), &spec.path);
                let mut m = MoonshineModel::load(&spec.path, variant, &quantization)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let _ =
                    m.transcribe_with(&vec![0.0; ms_to_samples(1000)], &MoonshineParams::default());
                (Kind::Moonshine(Box::new(m)), Device::Cpu, None)
            }
        };
        Ok(Self {
            spec,
            kind,
            wanted_gpu: use_gpu,
            device,
            gpu_name,
            last_used: Instant::now(),
        })
    }

    pub fn supports_translate(&self) -> bool {
        matches!(&self.kind, Kind::Whisper(w) if w.multilingual())
    }

    pub fn supports_language_id(&self) -> bool {
        self.supports_translate()
    }

    pub fn transcribe(
        &mut self,
        prepared: &speech::Prepared,
        req: &Request,
    ) -> anyhow::Result<Output> {
        self.last_used = Instant::now();
        match &mut self.kind {
            Kind::Whisper(w) => {
                let out = w.transcribe(
                    &prepared.audio,
                    &whisper::Job {
                        task: req.task,
                        language: req.language,
                        candidates: req.candidates,
                        prompt: req.prompt,
                        threads: req.threads,
                        beam_size: req.beam_size,
                        cancel: req.cancel.clone(),
                    },
                )?;
                Ok(Output {
                    text: out.text,
                    language: out.language,
                    language_probability: out.language_probability,
                    segments: out.segments,
                })
            }
            Kind::Parakeet(m) => {
                if req.task == Task::Translate {
                    anyhow::bail!(crate::Unsupported("Parakeet can't translate".into()));
                }
                let params = ParakeetParams {
                    language: None,
                    timestamp_granularity: Some(TimestampGranularity::Segment),
                };
                chunked(prepared, PARAKEET_CHUNK_MS, req, |audio| {
                    m.transcribe_with(audio, &params)
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
            }
            Kind::Moonshine(m) => {
                if req.task == Task::Translate {
                    anyhow::bail!(crate::Unsupported("Moonshine can't translate".into()));
                }
                chunked(prepared, MOONSHINE_CHUNK_MS, req, |audio| {
                    m.transcribe_with(audio, &MoonshineParams::default())
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
            }
        }
    }
}

fn chunked(
    prepared: &speech::Prepared,
    chunk_ms: u64,
    req: &Request,
    mut run: impl FnMut(&[f32]) -> anyhow::Result<transcribe_rs::TranscriptionResult>,
) -> anyhow::Result<Output> {
    let mut text = String::new();
    let mut segments = Vec::new();
    for range in speech::chunks(&prepared.audio, &prepared.breaks, ms_to_samples(chunk_ms)) {
        if req.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!(crate::Cancelled);
        }
        let offset_ms = joona_protocol::samples_to_ms(range.start);
        let result = run(&prepared.audio[range])?;
        let piece = result.text.trim();
        if piece.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(piece);
        for s in result.segments.unwrap_or_default() {
            segments.push(Segment {
                start_ms: offset_ms + (s.start.max(0.0) * 1000.0) as u64,
                end_ms: offset_ms + (s.end.max(0.0) * 1000.0) as u64,
                text: s.text.trim().to_string(),
            });
        }
    }
    Ok(Output {
        text,
        language: None,
        language_probability: None,
        segments,
    })
}

/// ONNX models ship in several precisions; int8 is the default download.
/// Missing files fall back to fp32 inside transcribe-rs.
fn quantization(variant: Option<&str>) -> Quantization {
    match variant.map(str::to_ascii_lowercase).as_deref() {
        Some(v) if v.contains("fp32") => Quantization::FP32,
        Some(v) if v.contains("fp16") => Quantization::FP16,
        _ => Quantization::Int8,
    }
}

fn moonshine_variant(variant: Option<&str>, path: &Path) -> MoonshineVariant {
    let name = variant
        .map(str::to_owned)
        .unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .to_ascii_lowercase();
    let base = name.contains("base");
    match () {
        _ if name.contains("-es") || name.contains("_es") => MoonshineVariant::BaseEs,
        _ if base => MoonshineVariant::Base,
        _ if name.contains("-ar") => MoonshineVariant::TinyAr,
        _ if name.contains("-zh") => MoonshineVariant::TinyZh,
        _ if name.contains("-ja") => MoonshineVariant::TinyJa,
        _ if name.contains("-ko") => MoonshineVariant::TinyKo,
        _ if name.contains("-uk") => MoonshineVariant::TinyUk,
        _ if name.contains("-vi") => MoonshineVariant::TinyVi,
        _ => MoonshineVariant::Tiny,
    }
}
