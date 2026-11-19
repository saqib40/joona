//! Messages exchanged between the Joona app and the `joona-engine` sidecar.
//!
//! The transport is newline-delimited JSON over the engine's stdin and stdout.
//! Every line the app writes is a [`Request`]. Every line the engine writes is
//! an [`Outgoing`]: either a [`Response`] to a request that carried an `id`, or
//! an [`Event`] the engine raises on its own. Requests without an `id` (such as
//! `feed`) are notifications and get no response.
//!
//! ```text
//! -> {"id":1,"method":"open","params":{"model":"whisper-small","task":"translate","language":"hi","prompt":"Priya, Joona"}}
//! -> {"method":"feed","params":{"session":1,"pcm16":"<base64, ~200 ms>"}}
//! -> {"id":2,"method":"finish","params":{"session":1}}
//! <- {"id":2,"result":{"text":"Let's meet at 3 tomorrow.","language":"hi","infer_ms":610,...}}
//! ```
//!
//! Audio is mono, 16 kHz, signed 16-bit little-endian PCM, base64 encoded. It is
//! only ever held in memory.

use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const SAMPLE_RATE: u32 = 16_000;

/// A line written by the app to the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub call: Call,
}

impl Request {
    pub fn call(id: u64, call: Call) -> Self {
        Self { id: Some(id), call }
    }

    pub fn notify(call: Call) -> Self {
        Self { id: None, call }
    }

    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("requests always serialize");
        line.push('\n');
        line
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    /// Handshake. Answered with [`EngineInfo`].
    Hello,
    /// Engine-wide settings. Answered with `null`.
    Configure(ConfigureParams),
    /// Loads a model into memory, replacing nothing else. Answered with [`LoadResult`].
    Load(LoadParams),
    /// Frees a model, or every model when `model` is `None`. Answered with `null`.
    Unload(UnloadParams),
    /// Starts a dictation session on a loaded model. Answered with [`OpenResult`].
    Open(OpenParams),
    /// Appends audio to a session. Usually sent as a notification.
    Feed(FeedParams),
    /// Ends the audio of a session and transcribes it. Answered with [`TranscribeResult`].
    Finish(SessionParams),
    /// Drops a session and aborts its transcription if one is running. Answered with `null`.
    Cancel(SessionParams),
    /// Answered with [`EngineStatus`].
    Status,
    /// Answered with `null`, then the engine exits.
    Shutdown,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigureParams {
    /// Silero VAD model used to trim silence before transcription.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vad_model: Option<PathBuf>,
    /// CPU threads for inference. `None` lets the engine decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    /// Whisper beam width: 1 is fastest, 5 is most accurate. `None` keeps the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beam_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Whisper,
    Parakeet,
    Moonshine,
}

impl EngineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineKind::Whisper => "whisper",
            EngineKind::Parakeet => "parakeet",
            EngineKind::Moonshine => "moonshine",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Catalog id, for example `whisper-small` or `parakeet-tdt-0.6b-v3`.
    pub id: String,
    pub engine: EngineKind,
    /// A file for Whisper (`ggml-*.bin`), a directory for ONNX models.
    pub path: PathBuf,
    /// Engine-specific variant: Moonshine `tiny`/`base`, ONNX quantization `int8`/`fp32`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadParams {
    pub model: ModelSpec,
    #[serde(default)]
    pub use_gpu: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadResult {
    pub model: String,
    pub device: Device,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_name: Option<String>,
    pub load_ms: u64,
    /// Whether the model can translate speech into English (multilingual Whisper only).
    pub supports_translate: bool,
    /// Whether the model can identify the spoken language.
    pub supports_language_id: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UnloadParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    /// Text in the spoken language.
    #[default]
    Transcribe,
    /// English text from any spoken language (Whisper's built-in translation).
    Translate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenParams {
    /// Id of a model loaded with `load`.
    pub model: String,
    #[serde(default)]
    pub task: Task,
    /// Spoken language as an ISO 639-1 code. `None` means detect it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// When detecting, only these languages are considered. Empty means any language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    /// Vocabulary hint: words from the user's dictionary, separated by
    /// commas. Whisper uses the ones written in the script it writes in;
    /// other models ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Trim silence with the configured VAD model before transcribing.
    #[serde(default = "default_true")]
    pub vad: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenResult {
    pub session: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedParams {
    pub session: u64,
    /// Base64 of mono 16 kHz signed 16-bit little-endian samples.
    pub pcm16: String,
}

impl FeedParams {
    pub fn from_samples(session: u64, samples: &[f32]) -> Self {
        Self {
            session,
            pcm16: encode_pcm16(samples),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionParams {
    pub session: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub text: String,
    /// Language the text was recognised in, as an ISO 639-1 code, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_probability: Option<f32>,
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// Length of the audio received.
    pub audio_ms: u64,
    /// Length of the audio left after silence trimming.
    pub speech_ms: u64,
    pub infer_ms: u64,
    pub model: String,
    pub device: Device,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    #[default]
    Cpu,
    Vulkan,
    Cuda,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineInfo {
    pub protocol: u32,
    pub version: String,
    pub engines: Vec<EngineKind>,
    /// GPU backend compiled into this build, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<Device>,
    /// GPUs that backend can use. Empty on CPU builds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpus: Vec<GpuInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
    /// A discrete card rather than graphics built into the CPU.
    pub dedicated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedModel {
    pub id: String,
    pub engine: EngineKind,
    pub device: Device,
    pub idle_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineStatus {
    pub loaded: Vec<LoadedModel>,
    pub sessions: u32,
    pub busy: bool,
}

/// A line written by the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Outgoing {
    Response(Response),
    Event(Event),
}

impl Outgoing {
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("engine output always serializes");
        line.push('\n');
        line
    }

    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok<T: Serialize>(id: u64, value: &T) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(value).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }

    pub fn err(id: u64, error: RpcError) -> Self {
        Self {
            id,
            result: None,
            error: Some(error),
        }
    }

    pub fn into_result<T: serde::de::DeserializeOwned>(self) -> Result<T, RpcError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let value = self.result.unwrap_or(serde_json::Value::Null);
        serde_json::from_value(value).map_err(|e| RpcError::new(ErrorCode::Protocol, e.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The line could not be parsed or had the wrong shape.
    Protocol,
    ModelNotLoaded,
    ModelLoadFailed,
    UnknownSession,
    /// The model cannot do what was asked, for example translate with an English-only model.
    Unsupported,
    Cancelled,
    InferenceFailed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "params", rename_all = "snake_case")]
pub enum Event {
    /// Sent once after start-up, before any request is read.
    Ready(EngineInfo),
    Log(LogEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEvent {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Converts float samples in [-1, 1] to base64 PCM16.
pub fn encode_pcm16(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    BASE64.encode(bytes)
}

/// Decodes base64 PCM16 into float samples, appending to `out`.
pub fn decode_pcm16_into(data: &str, out: &mut Vec<f32>) -> Result<(), base64::DecodeError> {
    let bytes = BASE64.decode(data)?;
    out.reserve(bytes.len() / 2);
    for &pair in bytes.as_chunks::<2>().0 {
        out.push(i16::from_le_bytes(pair) as f32 / i16::MAX as f32);
    }
    Ok(())
}

pub fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64 * 1000) / SAMPLE_RATE as u64
}

pub fn ms_to_samples(ms: u64) -> usize {
    (ms * SAMPLE_RATE as u64 / 1000) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_wire_format_matches_the_documented_shape() {
        let req = Request::call(
            1,
            Call::Open(OpenParams {
                model: "whisper-small".into(),
                task: Task::Translate,
                language: Some("hi".into()),
                candidates: vec![],
                prompt: Some("Priya, Joona".into()),
                vad: true,
            }),
        );
        let json: serde_json::Value = serde_json::from_str(&req.to_line()).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "open");
        assert_eq!(json["params"]["model"], "whisper-small");
        assert_eq!(json["params"]["task"], "translate");
        assert_eq!(json["params"]["language"], "hi");
    }

    #[test]
    fn documented_lines_parse() {
        let open: Request = serde_json::from_str(
            r#"{"id":1,"method":"open","params":{"model":"whisper-small","task":"translate","language":"hi","prompt":"Priya, Joona"}}"#,
        )
        .unwrap();
        assert_eq!(open.id, Some(1));
        match open.call {
            Call::Open(p) => {
                assert_eq!(p.task, Task::Translate);
                assert!(p.vad, "vad defaults to on");
            }
            other => panic!("unexpected call {other:?}"),
        }

        let feed: Request =
            serde_json::from_str(r#"{"method":"feed","params":{"session":1,"pcm16":"AAA="}}"#)
                .unwrap();
        assert_eq!(feed.id, None);
        assert!(matches!(feed.call, Call::Feed(_)));

        let hello: Request = serde_json::from_str(r#"{"id":7,"method":"hello"}"#).unwrap();
        assert_eq!(hello.call, Call::Hello);
    }

    #[test]
    fn unit_calls_round_trip() {
        for call in [Call::Hello, Call::Status, Call::Shutdown] {
            let line = Request::call(3, call.clone()).to_line();
            let back: Request = serde_json::from_str(&line).unwrap();
            assert_eq!(back.call, call);
        }
    }

    #[test]
    fn outgoing_distinguishes_responses_and_events() {
        let resp = Outgoing::Response(Response::ok(
            2,
            &serde_json::json!({"text": "hi", "audio_ms": 1, "speech_ms": 1, "infer_ms": 1, "model": "m", "device": "cpu"}),
        ));
        let back = Outgoing::parse(resp.to_line().trim()).unwrap();
        assert!(matches!(back, Outgoing::Response(r) if r.id == 2));

        let ev = Outgoing::Event(Event::Log(LogEvent {
            level: LogLevel::Info,
            message: "loaded".into(),
        }));
        let back = Outgoing::parse(ev.to_line().trim()).unwrap();
        assert!(matches!(back, Outgoing::Event(Event::Log(_))));
    }

    #[test]
    fn error_responses_surface_as_errors() {
        let resp = Response::err(5, RpcError::new(ErrorCode::ModelNotLoaded, "whisper-small"));
        let line = Outgoing::Response(resp).to_line();
        let Outgoing::Response(back) = Outgoing::parse(line.trim()).unwrap() else {
            panic!("expected a response");
        };
        let err = back.into_result::<TranscribeResult>().unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelNotLoaded);
    }

    #[test]
    fn pcm16_round_trip_is_close() {
        let samples: Vec<f32> = (0..1600).map(|i| ((i as f32) * 0.01).sin() * 0.8).collect();
        let encoded = encode_pcm16(&samples);
        let mut decoded = Vec::new();
        decode_pcm16_into(&encoded, &mut decoded).unwrap();
        assert_eq!(decoded.len(), samples.len());
        for (a, b) in samples.iter().zip(&decoded) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn pcm16_clamps_out_of_range_samples() {
        let mut decoded = Vec::new();
        decode_pcm16_into(&encode_pcm16(&[2.0, -2.0]), &mut decoded).unwrap();
        assert!((decoded[0] - 1.0).abs() < 1e-4);
        assert!((decoded[1] + 1.0).abs() < 1e-4);
    }
}
