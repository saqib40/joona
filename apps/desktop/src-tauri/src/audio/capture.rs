//! Microphone capture with cpal (WASAPI on Windows).
//!
//! The stream lives on its own thread for its whole life, because cpal streams
//! aren't `Send` on every platform. The device callback only downmixes to mono
//! and hands the samples over; resampling and level metering happen on the
//! capture thread.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde::Serialize;
use specta::Type;

use super::level::LevelMeter;
use super::resample::{StreamResampler, TARGET_RATE, downmix_into};

/// Receives 16 kHz mono audio and levels from the capture thread.
pub trait AudioSink: Send + 'static {
    fn on_audio(&mut self, samples: &[f32]);
    fn on_level(&mut self, level: f32);
    fn on_error(&mut self, message: String);
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let name = d.description().ok()?.name().to_string();
            Some(InputDevice {
                is_default: Some(&id) == default_id.as_ref(),
                id,
                name,
            })
        })
        .collect()
}

enum Msg {
    Audio(Vec<f32>),
    Error(String),
    Stop,
}

pub struct Capture {
    tx: mpsc::Sender<Msg>,
    join: Option<JoinHandle<()>>,
    pub info: CaptureInfo,
}

impl Capture {
    /// Opens the microphone (`device_id` from [`list_input_devices`], or the
    /// system default) and starts delivering audio to `sink`.
    pub fn start(device_id: Option<&str>, sink: Box<dyn AudioSink>) -> anyhow::Result<Capture> {
        let (tx, rx) = mpsc::channel::<Msg>();
        let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<CaptureInfo>>();
        let device_id = device_id.map(str::to_owned);
        let stream_tx = tx.clone();

        let join = std::thread::Builder::new()
            .name("joona-capture".into())
            .spawn(move || {
                let opened = open_stream(device_id.as_deref(), stream_tx);
                let (stream, info) = match opened {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ =
                        ready_tx.send(Err(anyhow::anyhow!("could not start the microphone: {e}")));
                    return;
                }
                let rate = info.sample_rate;
                let _ = ready_tx.send(Ok(info));
                pump(rx, rate, sink);
                drop(stream);
            })?;

        let info = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow::anyhow!("the microphone did not respond"))??;
        Ok(Capture {
            tx,
            join: Some(join),
            info,
        })
    }

    /// Stops recording. Audio still buffered is resampled and delivered first.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pump(rx: mpsc::Receiver<Msg>, rate: u32, mut sink: Box<dyn AudioSink>) {
    let mut resampler = match StreamResampler::new(rate) {
        Ok(r) => r,
        Err(e) => {
            sink.on_error(format!("unsupported sample rate {rate}: {e}"));
            return;
        }
    };
    // About 30 level updates per second.
    let mut meter = LevelMeter::new((TARGET_RATE / 30) as usize);
    let mut out = Vec::with_capacity(4096);
    loop {
        match rx.recv() {
            Ok(Msg::Audio(mono)) => {
                out.clear();
                resampler.push(&mono, &mut out);
                if !out.is_empty() {
                    meter.push(&out, |l| sink.on_level(l));
                    sink.on_audio(&out);
                }
            }
            Ok(Msg::Error(e)) => {
                sink.on_error(e);
                return;
            }
            Ok(Msg::Stop) | Err(_) => {
                // Drain what the callback already sent.
                while let Ok(Msg::Audio(mono)) = rx.try_recv() {
                    out.clear();
                    resampler.push(&mono, &mut out);
                    sink.on_audio(&out);
                }
                out.clear();
                resampler.finish(&mut out);
                if !out.is_empty() {
                    sink.on_audio(&out);
                }
                return;
            }
        }
    }
}

fn find_device(host: &cpal::Host, device_id: Option<&str>) -> anyhow::Result<cpal::Device> {
    if let Some(id) = device_id
        && let Ok(parsed) = id.parse::<cpal::DeviceId>()
        && let Some(device) = host.device_by_id(&parsed)
    {
        return Ok(device);
    }
    host.default_input_device().ok_or_else(|| {
        anyhow::anyhow!(
            "no microphone found. Check that one is connected and enabled in Windows sound settings"
        )
    })
}

fn open_stream(
    device_id: Option<&str>,
    tx: mpsc::Sender<Msg>,
) -> anyhow::Result<(cpal::Stream, CaptureInfo)> {
    let host = cpal::default_host();
    let device = find_device(&host, device_id)?;
    let supported = device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let format = supported.sample_format();
    let config = supported.config();
    let info = CaptureInfo {
        device_name: device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "Microphone".into()),
        sample_rate: config.sample_rate,
        channels: config.channels,
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, tx),
        SampleFormat::I16 => build::<i16>(&device, config, tx),
        SampleFormat::I32 => build::<i32>(&device, config, tx),
        SampleFormat::U16 => build::<u16>(&device, config, tx),
        SampleFormat::U8 => build::<u8>(&device, config, tx),
        SampleFormat::I8 => build::<i8>(&device, config, tx),
        SampleFormat::F64 => build::<f64>(&device, config, tx),
        other => Err(anyhow::anyhow!(
            "unsupported microphone sample format {other:?}"
        )),
    }?;
    Ok((stream, info))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    tx: mpsc::Sender<Msg>,
) -> anyhow::Result<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let err_tx = tx.clone();
    let stream = device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                downmix_into(data.iter().map(|s| f32::from_sample_(*s)), channels, &mut mono);
                let _ = tx.send(Msg::Audio(mono));
            },
            move |err| {
                let message = match err.kind() {
                    cpal::ErrorKind::DeviceNotAvailable => "the microphone was disconnected".to_string(),
                    cpal::ErrorKind::PermissionDenied => {
                        "Windows blocked microphone access. Allow it in Settings > Privacy & security > Microphone"
                            .to_string()
                    }
                    cpal::ErrorKind::Xrun => return,
                    _ => format!("microphone error: {err}"),
                };
                let _ = err_tx.send(Msg::Error(message));
            },
            Some(Duration::from_secs(3)),
        )
        .map_err(|e| match e.kind() {
            cpal::ErrorKind::DeviceBusy => anyhow::anyhow!("the microphone is busy in another app"),
            cpal::ErrorKind::PermissionDenied => anyhow::anyhow!(
                "Windows blocked microphone access. Allow it in Settings > Privacy & security > Microphone"
            ),
            _ => anyhow::anyhow!("could not open the microphone: {e}"),
        })?;
    Ok(stream)
}
