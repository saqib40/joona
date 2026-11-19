//! joona-engine: local speech-to-text for Joona, run as a sidecar process.
//!
//! Requests arrive as JSON lines on stdin and answers leave as JSON lines on
//! stdout (see `joona-protocol`). Native libraries log to stderr, which the
//! app captures. The stdin thread handles quick calls itself, including the
//! audio feed and cancellation, so they never wait behind a transcription. A
//! worker thread owns the models and runs loads and transcriptions in order.

mod models;
mod speech;
mod text;
mod whisper;

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::Instant;

use joona_protocol::{
    Call, ConfigureParams, Device, EngineInfo, EngineKind, EngineStatus, ErrorCode, Event,
    FeedParams, GpuInfo, LoadParams, LoadResult, LoadedModel, LogEvent, LogLevel, OpenParams,
    OpenResult, Outgoing, PROTOCOL_VERSION, Request, Response, RpcError, SAMPLE_RATE,
    TranscribeResult, UnloadParams, decode_pcm16_into, samples_to_ms,
};
use serde::Serialize;

pub const GPU_BACKEND: Option<Device> = if cfg!(feature = "cuda") {
    Some(Device::Cuda)
} else if cfg!(feature = "vulkan") {
    Some(Device::Vulkan)
} else {
    None
};

/// Audio beyond this is dropped. The app stops dictation at 20 minutes.
const MAX_SESSION_SAMPLES: usize = 25 * 60 * SAMPLE_RATE as usize;
const DEFAULT_BEAM: i32 = 3;

/// The loaded model can't do what was asked.
#[derive(Debug)]
pub struct Unsupported(pub String);

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Unsupported {}

#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Output {
    stdout: Mutex<std::io::Stdout>,
}

impl Output {
    fn send(&self, msg: &Outgoing) {
        let line = msg.to_line();
        let mut out = lock(&self.stdout);
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
    }

    /// Answers a request. Notifications (no id) get no answer; their errors are logged.
    fn reply<T: Serialize>(&self, id: Option<u64>, result: Result<T, RpcError>) {
        match (id, result) {
            (Some(id), Ok(v)) => self.send(&Outgoing::Response(Response::ok(id, &v))),
            (Some(id), Err(e)) => self.send(&Outgoing::Response(Response::err(id, e))),
            (None, Err(e)) => self.log(LogLevel::Warn, e.to_string()),
            (None, Ok(_)) => {}
        }
    }

    fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.send(&Outgoing::Event(Event::Log(LogEvent {
            level,
            message: message.into(),
        })));
    }
}

struct Session {
    params: OpenParams,
    audio: Vec<f32>,
    scores: Option<speech::Scores>,
    cancel: Arc<AtomicBool>,
}

struct LoadedInfo {
    engine: EngineKind,
    device: Device,
    last_used: Instant,
}

struct Shared {
    out: Output,
    sessions: Mutex<HashMap<u64, Session>>,
    /// Cancel flags of sessions handed to the worker.
    running: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    loaded: Mutex<HashMap<String, LoadedInfo>>,
    detector: Mutex<Option<speech::Detector>>,
    busy: AtomicBool,
    next_session: AtomicU64,
    threads: AtomicI32,
    beam_size: AtomicI32,
}

enum Job {
    Load {
        id: Option<u64>,
        params: LoadParams,
    },
    Unload {
        id: Option<u64>,
        params: UnloadParams,
    },
    Finish {
        id: Option<u64>,
        session_id: u64,
        session: Session,
    },
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("joona-engine crashed: {info}");
        std::process::exit(101);
    }));
    quiet_native_logs();

    let shared = Arc::new(Shared {
        out: Output {
            stdout: Mutex::new(std::io::stdout()),
        },
        sessions: Mutex::new(HashMap::new()),
        running: Mutex::new(HashMap::new()),
        loaded: Mutex::new(HashMap::new()),
        detector: Mutex::new(None),
        busy: AtomicBool::new(false),
        next_session: AtomicU64::new(0),
        threads: AtomicI32::new(default_threads()),
        beam_size: AtomicI32::new(DEFAULT_BEAM),
    });

    let (jobs, rx) = mpsc::channel::<Job>();
    let worker_shared = shared.clone();
    std::thread::Builder::new()
        .name("joona-engine-worker".into())
        .spawn(move || worker(&worker_shared, rx))
        .expect("could not start the worker thread");

    shared
        .out
        .send(&Outgoing::Event(Event::Ready(engine_info())));

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let id = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|v| v.get("id").and_then(|i| i.as_u64()));
                let err =
                    RpcError::new(ErrorCode::Protocol, format!("could not read request: {e}"));
                match id {
                    Some(_) => shared.out.reply::<()>(id, Err(err)),
                    None => shared.out.log(LogLevel::Warn, err.message),
                }
                continue;
            }
        };
        if !handle(&shared, &jobs, req) {
            break;
        }
    }
    // The app closed stdin or asked us to stop. Nothing needs saving.
    std::process::exit(0);
}

/// Returns false when the engine should exit.
fn handle(shared: &Shared, jobs: &mpsc::Sender<Job>, req: Request) -> bool {
    let id = req.id;
    match req.call {
        Call::Hello => shared.out.reply(id, Ok(engine_info())),
        Call::Configure(p) => shared.out.reply(id, configure(shared, p)),
        Call::Load(params) => {
            let _ = jobs.send(Job::Load { id, params });
        }
        Call::Unload(params) => {
            let _ = jobs.send(Job::Unload { id, params });
        }
        Call::Open(p) => shared.out.reply(id, open(shared, p)),
        Call::Feed(p) => shared.out.reply(id, feed(shared, p)),
        Call::Finish(p) => {
            let taken = lock(&shared.sessions).remove(&p.session);
            match taken {
                Some(mut session) => {
                    if let Some(scores) = &mut session.scores
                        && let Some(det) = lock(&shared.detector).as_mut()
                    {
                        scores.finish(det, &session.audio);
                    }
                    lock(&shared.running).insert(p.session, session.cancel.clone());
                    let _ = jobs.send(Job::Finish {
                        id,
                        session_id: p.session,
                        session,
                    });
                }
                None => shared.out.reply::<()>(id, Err(unknown_session(p.session))),
            }
        }
        Call::Cancel(p) => {
            lock(&shared.sessions).remove(&p.session);
            if let Some(flag) = lock(&shared.running).get(&p.session) {
                flag.store(true, Ordering::Relaxed);
            }
            shared.out.reply(id, Ok(()));
        }
        Call::Status => shared.out.reply(id, Ok(status(shared))),
        Call::Shutdown => {
            shared.out.reply(id, Ok(()));
            return false;
        }
    }
    true
}

fn unknown_session(session: u64) -> RpcError {
    RpcError::new(
        ErrorCode::UnknownSession,
        format!("no open session {session}"),
    )
}

fn configure(shared: &Shared, p: ConfigureParams) -> Result<(), RpcError> {
    if let Some(threads) = p.threads {
        shared
            .threads
            .store(threads.clamp(1, 64) as i32, Ordering::Relaxed);
    }
    if let Some(beam) = p.beam_size {
        shared
            .beam_size
            .store(beam.clamp(1, 8) as i32, Ordering::Relaxed);
    }
    if let Some(path) = p.vad_model {
        let detector = speech::Detector::load(&path)
            .map_err(|e| RpcError::new(ErrorCode::ModelLoadFailed, format!("{e:#}")))?;
        *lock(&shared.detector) = Some(detector);
    }
    Ok(())
}

fn open(shared: &Shared, p: OpenParams) -> Result<OpenResult, RpcError> {
    if !lock(&shared.loaded).contains_key(&p.model) {
        return Err(RpcError::new(
            ErrorCode::ModelNotLoaded,
            format!("{} is not loaded", p.model),
        ));
    }
    let scores = if p.vad {
        lock(&shared.detector).as_mut().map(|det| {
            det.reset();
            speech::Scores::default()
        })
    } else {
        None
    };
    let session = shared.next_session.fetch_add(1, Ordering::Relaxed) + 1;
    lock(&shared.sessions).insert(
        session,
        Session {
            params: p,
            audio: Vec::with_capacity(SAMPLE_RATE as usize * 30),
            scores,
            cancel: Arc::new(AtomicBool::new(false)),
        },
    );
    Ok(OpenResult { session })
}

fn feed(shared: &Shared, p: FeedParams) -> Result<(), RpcError> {
    let mut sessions = lock(&shared.sessions);
    let s = sessions
        .get_mut(&p.session)
        .ok_or_else(|| unknown_session(p.session))?;
    if s.audio.len() >= MAX_SESSION_SAMPLES {
        return Ok(());
    }
    decode_pcm16_into(&p.pcm16, &mut s.audio)
        .map_err(|e| RpcError::new(ErrorCode::Protocol, format!("bad audio data: {e}")))?;
    if let Some(scores) = &mut s.scores
        && let Some(det) = lock(&shared.detector).as_mut()
    {
        scores.update(det, &s.audio);
    }
    Ok(())
}

fn status(shared: &Shared) -> EngineStatus {
    let loaded = lock(&shared.loaded)
        .iter()
        .map(|(id, info)| LoadedModel {
            id: id.clone(),
            engine: info.engine,
            device: info.device,
            idle_ms: info.last_used.elapsed().as_millis() as u64,
        })
        .collect();
    EngineStatus {
        loaded,
        sessions: lock(&shared.sessions).len() as u32,
        busy: shared.busy.load(Ordering::Relaxed),
    }
}

fn worker(shared: &Shared, rx: mpsc::Receiver<Job>) {
    let mut models: HashMap<String, models::Model> = HashMap::new();
    for job in rx {
        match job {
            Job::Load { id, params } => {
                let result = load(shared, &mut models, params);
                shared.out.reply(id, result);
            }
            Job::Unload { id, params } => {
                match params.model {
                    Some(m) => {
                        models.remove(&m);
                        lock(&shared.loaded).remove(&m);
                    }
                    None => {
                        models.clear();
                        lock(&shared.loaded).clear();
                    }
                }
                shared.out.reply(id, Ok(()));
            }
            Job::Finish {
                id,
                session_id,
                session,
            } => {
                shared.busy.store(true, Ordering::Relaxed);
                let result = transcribe(shared, &mut models, session);
                shared.busy.store(false, Ordering::Relaxed);
                lock(&shared.running).remove(&session_id);
                shared.out.reply(id, result);
            }
        }
    }
}

fn load(
    shared: &Shared,
    models: &mut HashMap<String, models::Model>,
    params: LoadParams,
) -> Result<LoadResult, RpcError> {
    let id = params.model.id.clone();
    let describe = |m: &models::Model, load_ms: u64| LoadResult {
        model: m.spec.id.clone(),
        device: m.device,
        gpu_name: m.gpu_name.clone(),
        load_ms,
        supports_translate: m.supports_translate(),
        supports_language_id: m.supports_language_id(),
    };
    if let Some(m) = models.get(&id)
        && m.spec == params.model
        && m.wanted_gpu == params.use_gpu
    {
        return Ok(describe(m, 0));
    }
    models.remove(&id);
    lock(&shared.loaded).remove(&id);

    let threads = shared.threads.load(Ordering::Relaxed);
    let started = Instant::now();
    let model = match models::Model::load(params.model.clone(), params.use_gpu, threads) {
        Ok(m) => m,
        Err(e) if params.use_gpu => {
            shared.out.log(
                LogLevel::Warn,
                format!("GPU load of {id} failed, using the CPU: {e:#}"),
            );
            models::Model::load(params.model, false, threads)
                .map_err(|e| RpcError::new(ErrorCode::ModelLoadFailed, format!("{e:#}")))?
        }
        Err(e) => return Err(RpcError::new(ErrorCode::ModelLoadFailed, format!("{e:#}"))),
    };
    let result = describe(&model, started.elapsed().as_millis() as u64);
    lock(&shared.loaded).insert(
        id.clone(),
        LoadedInfo {
            engine: model.spec.engine,
            device: model.device,
            last_used: Instant::now(),
        },
    );
    shared.out.log(
        LogLevel::Info,
        format!("loaded {id} on {:?} in {} ms", model.device, result.load_ms),
    );
    models.insert(id, model);
    Ok(result)
}

fn transcribe(
    shared: &Shared,
    models: &mut HashMap<String, models::Model>,
    mut session: Session,
) -> Result<TranscribeResult, RpcError> {
    let started = Instant::now();
    let model = models.get_mut(&session.params.model).ok_or_else(|| {
        RpcError::new(
            ErrorCode::ModelNotLoaded,
            format!("{} was unloaded", session.params.model),
        )
    })?;
    let audio_ms = samples_to_ms(session.audio.len());
    let probs = session.scores.as_ref().map(|s| s.probs.as_slice());
    let prepared = speech::prepare(&session.audio, probs);
    let had_vad = probs.is_some();
    session.audio = Vec::new();

    let mut result = TranscribeResult {
        text: String::new(),
        language: session.params.language.clone(),
        language_probability: None,
        segments: Vec::new(),
        audio_ms,
        speech_ms: prepared.speech_ms,
        infer_ms: 0,
        model: model.spec.id.clone(),
        device: model.device,
    };
    if prepared.audio.is_empty() {
        return Ok(result);
    }

    let request = models::Request {
        task: session.params.task,
        language: session.params.language.as_deref(),
        candidates: &session.params.candidates,
        prompt: session.params.prompt.as_deref(),
        threads: shared.threads.load(Ordering::Relaxed),
        beam_size: shared.beam_size.load(Ordering::Relaxed),
        cancel: session.cancel.clone(),
    };
    let out = model.transcribe(&prepared, &request).map_err(|e| {
        if e.is::<Cancelled>() {
            RpcError::new(ErrorCode::Cancelled, "cancelled")
        } else if let Some(u) = e.downcast_ref::<Unsupported>() {
            RpcError::new(ErrorCode::Unsupported, u.0.clone())
        } else {
            RpcError::new(ErrorCode::InferenceFailed, format!("{e:#}"))
        }
    })?;
    if session.cancel.load(Ordering::Relaxed) {
        return Err(RpcError::new(ErrorCode::Cancelled, "cancelled"));
    }
    if let Some(info) = lock(&shared.loaded).get_mut(&model.spec.id) {
        info.last_used = Instant::now();
    }

    result.text = text::clean(&out.text, had_vad);
    result.language = out.language.or(result.language);
    result.language_probability = out.language_probability;
    result.segments = out.segments;
    result.infer_ms = started.elapsed().as_millis() as u64;
    Ok(result)
}

fn engine_info() -> EngineInfo {
    EngineInfo {
        protocol: PROTOCOL_VERSION,
        version: env!("CARGO_PKG_VERSION").to_string(),
        engines: vec![
            EngineKind::Whisper,
            EngineKind::Parakeet,
            EngineKind::Moonshine,
        ],
        gpu: GPU_BACKEND,
        gpus: gpus(),
    }
}

fn gpus() -> Vec<GpuInfo> {
    if GPU_BACKEND.is_none() {
        return Vec::new();
    }
    use transcribe_rs::whisper_cpp::gpu::{GpuKind, list_gpu_devices};
    list_gpu_devices()
        .into_iter()
        .map(|g| GpuInfo {
            name: g.name,
            vram_mb: (g.total_vram / (1024 * 1024)) as u64,
            dedicated: g.kind == GpuKind::Dedicated,
        })
        .collect()
}

/// whisper.cpp and ggml log every decoding step by default. Only messages at
/// info level and above reach stderr, which the app keeps in its log file.
fn quiet_native_logs() {
    use std::ffi::{CStr, c_char, c_void};
    use whisper_rs::whisper_rs_sys as sys;

    static LAST_SHOWN: AtomicBool = AtomicBool::new(false);

    unsafe extern "C" fn log(level: sys::ggml_log_level, text: *const c_char, _: *mut c_void) {
        let show = if level == sys::ggml_log_level_GGML_LOG_LEVEL_CONT {
            LAST_SHOWN.load(Ordering::Relaxed)
        } else {
            let show = level >= sys::ggml_log_level_GGML_LOG_LEVEL_INFO;
            LAST_SHOWN.store(show, Ordering::Relaxed);
            show
        };
        if show && !text.is_null() {
            // SAFETY: ggml passes a NUL-terminated string that lives for the call.
            eprint!("{}", unsafe { CStr::from_ptr(text) }.to_string_lossy());
        }
    }

    // SAFETY: the callback is a plain function that only reads its arguments.
    unsafe {
        sys::whisper_log_set(Some(log), std::ptr::null_mut());
        sys::ggml_log_set(Some(log), std::ptr::null_mut());
    }
}

/// Whisper scales with physical cores, and most CPUs run two threads per core.
fn default_threads() -> i32 {
    let logical = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let physical = if logical >= 8 { logical / 2 } else { logical };
    physical.clamp(2, 8) as i32
}
