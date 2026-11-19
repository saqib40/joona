//! Runs the `joona-engine` sidecar. The supervisor finds the engine binary,
//! starts it hidden inside a kill-on-close job, matches its responses to
//! requests, and restarts it after a crash with the same settings and models.
//!
//! The engine starts on first use. Sessions belong to the engine process that
//! opened them: if it crashes, their calls fail with [`EngineError::Stopped`]
//! and the caller retries on a new session with the audio it kept.

pub(crate) mod job;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use joona_protocol::{
    Call, ConfigureParams, EngineInfo, EngineStatus, Event, FeedParams, LoadParams, LoadResult,
    LogEvent, LogLevel, ModelSpec, OpenParams, OpenResult, Outgoing, PROTOCOL_VERSION, Request,
    Response, RpcError, SessionParams, TranscribeResult, UnloadParams,
};
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, mpsc, oneshot};

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// Large models on a slow disk, plus GPU shader compilation on first use.
const LOAD_TIMEOUT: Duration = Duration::from_secs(300);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
/// Restart delays after consecutive crashes. After the last one the engine
/// stays down until something needs it again.
const BACKOFF: [Duration; 5] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];
/// Crashes older than this no longer count towards the backoff.
const CRASH_WINDOW: Duration = Duration::from_secs(300);
const STDERR_TAIL: usize = 40;
const STATUS_ILLEGAL_INSTRUCTION: u32 = 0xC000_001D;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("Joona's local engine is missing ({0}). Reinstall Joona or choose a cloud provider.")]
    Missing(String),
    #[error(
        "This processor lacks AVX2, which local transcription needs. Choose a cloud provider in Settings."
    )]
    CpuUnsupported,
    #[error("Couldn't start the local engine: {0}")]
    Spawn(String),
    #[error("The local engine stopped unexpectedly.")]
    Stopped,
    #[error("The local engine didn't respond in time.")]
    Timeout,
    #[error(
        "The local engine speaks protocol {0}, but this version of Joona needs {PROTOCOL_VERSION}."
    )]
    Incompatible(u32),
    #[error(transparent)]
    Rpc(RpcError),
}

/// Which engine build to run. The Vulkan build is an optional download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Cpu,
    Vulkan,
}

impl Variant {
    pub fn is_installed(self) -> bool {
        engine_exe(self).is_ok()
    }

    fn binary_name(self) -> String {
        let stem = match self {
            Variant::Cpu => "joona-engine",
            Variant::Vulkan => "joona-engine-vulkan",
        };
        format!("{stem}{}", std::env::consts::EXE_SUFFIX)
    }
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Started {
        variant: Variant,
        info: EngineInfo,
    },
    /// `restart_in` is `None` when the engine crashed too often and stays down.
    Crashed {
        message: String,
        restart_in: Option<Duration>,
    },
    FellBackToCpu {
        reason: String,
    },
}

/// How long to wait for a transcription. A slow CPU running a large model can
/// take longer than the audio itself.
pub fn finish_timeout(audio_ms: u64) -> Duration {
    Duration::from_millis(30_000 + audio_ms.saturating_mul(3))
}

#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<Inner>,
}

struct Inner {
    conn: tokio::sync::Mutex<Option<Arc<Conn>>>,
    replay: StdMutex<Replay>,
    variant: StdMutex<Variant>,
    events: broadcast::Sender<EngineEvent>,
    next_id: AtomicU64,
    crashes: StdMutex<VecDeque<Instant>>,
    stopping: AtomicBool,
    job: Option<job::KillOnClose>,
}

/// What a restarted engine needs to be given again.
#[derive(Debug, Clone, Default)]
struct Replay {
    configure: Option<ConfigureParams>,
    models: Vec<LoadParams>,
}

struct Conn {
    lines: mpsc::UnboundedSender<String>,
    pending: StdMutex<HashMap<u64, oneshot::Sender<Response>>>,
    info: EngineInfo,
    variant: Variant,
    alive: AtomicBool,
    /// The model being loaded, so a crash during the load can be blamed on it.
    loading: StdMutex<Option<String>>,
    stderr: Arc<StdMutex<VecDeque<String>>>,
    child: StdMutex<Option<Child>>,
}

impl Supervisor {
    pub fn new(variant: Variant) -> Self {
        let job = job::KillOnClose::new();
        if job.is_none() {
            tracing::warn!("couldn't create a job object; the engine may outlive a crashed Joona");
        }
        Self {
            inner: Arc::new(Inner {
                conn: tokio::sync::Mutex::new(None),
                replay: StdMutex::new(Replay::default()),
                variant: StdMutex::new(variant),
                events: broadcast::channel(32).0,
                next_id: AtomicU64::new(1),
                crashes: StdMutex::new(VecDeque::new()),
                stopping: AtomicBool::new(false),
                job,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    pub fn variant(&self) -> Variant {
        *self.inner.variant.lock().unwrap()
    }

    /// Switches engine builds. A running engine is stopped, and the next call
    /// starts the new build with the same settings and models.
    pub async fn set_variant(&self, variant: Variant) {
        if self.variant() == variant {
            return;
        }
        *self.inner.variant.lock().unwrap() = variant;
        self.stop().await;
    }

    /// Starts the engine if it isn't running.
    pub async fn start(&self) -> Result<EngineInfo, EngineError> {
        Ok(self.inner.connection().await?.info.clone())
    }

    /// Details of the running engine, without starting it.
    pub async fn info(&self) -> Option<EngineInfo> {
        self.inner.current().await.map(|conn| conn.info.clone())
    }

    pub async fn configure(&self, params: ConfigureParams) -> Result<(), EngineError> {
        let conn = self.inner.connection().await?;
        conn.call::<()>(
            self.inner.id(),
            Call::Configure(params.clone()),
            CALL_TIMEOUT,
        )
        .await?;
        self.inner.replay.lock().unwrap().configure = Some(params);
        Ok(())
    }

    /// Stores engine settings for every start, and applies them now if the
    /// engine is running. Unlike [`Supervisor::configure`], never starts it.
    pub async fn set_config(&self, params: ConfigureParams) -> Result<(), EngineError> {
        self.inner.replay.lock().unwrap().configure = Some(params.clone());
        if let Some(conn) = self.inner.current().await {
            conn.call::<()>(self.inner.id(), Call::Configure(params), CALL_TIMEOUT)
                .await?;
        }
        Ok(())
    }

    /// Loads a model, or confirms it is loaded. Cheap when nothing changed.
    pub async fn load(&self, model: ModelSpec, use_gpu: bool) -> Result<LoadResult, EngineError> {
        let conn = self.inner.connection().await?;
        let params = LoadParams { model, use_gpu };
        let result = conn.load(self.inner.id(), params.clone()).await?;
        let mut replay = self.inner.replay.lock().unwrap();
        replay.models.retain(|m| m.model.id != params.model.id);
        replay.models.push(params);
        Ok(result)
    }

    /// Frees a model, or all of them when `model` is `None`.
    pub async fn unload(&self, model: Option<String>) -> Result<(), EngineError> {
        {
            let mut replay = self.inner.replay.lock().unwrap();
            match &model {
                Some(id) => replay.models.retain(|m| &m.model.id != id),
                None => replay.models.clear(),
            }
        }
        let Some(conn) = self.inner.current().await else {
            return Ok(());
        };
        conn.call(
            self.inner.id(),
            Call::Unload(UnloadParams { model }),
            CALL_TIMEOUT,
        )
        .await
    }

    /// Frees models that haven't transcribed anything for `idle`.
    pub async fn unload_idle(&self, idle: Duration) -> Result<Vec<String>, EngineError> {
        let Some(conn) = self.inner.current().await else {
            return Ok(Vec::new());
        };
        let status: EngineStatus = conn
            .call(self.inner.id(), Call::Status, CALL_TIMEOUT)
            .await?;
        if status.busy || status.sessions > 0 {
            return Ok(Vec::new());
        }
        let mut freed = Vec::new();
        for model in status.loaded {
            if Duration::from_millis(model.idle_ms) >= idle {
                self.unload(Some(model.id.clone())).await?;
                tracing::info!(model = %model.id, "unloaded an idle model");
                freed.push(model.id);
            }
        }
        Ok(freed)
    }

    pub async fn open(&self, params: OpenParams) -> Result<Session, EngineError> {
        let conn = self.inner.connection().await?;
        let opened: OpenResult = conn
            .call(self.inner.id(), Call::Open(params), CALL_TIMEOUT)
            .await?;
        Ok(Session {
            inner: self.inner.clone(),
            conn,
            id: opened.session,
            done: false,
        })
    }

    pub async fn status(&self) -> Result<EngineStatus, EngineError> {
        let conn = self.inner.connection().await?;
        conn.call(self.inner.id(), Call::Status, CALL_TIMEOUT).await
    }

    /// The running engine's status. Unlike [`Supervisor::status`], never starts it.
    pub async fn status_if_running(&self) -> Option<EngineStatus> {
        let conn = self.inner.current().await?;
        conn.call(self.inner.id(), Call::Status, CALL_TIMEOUT)
            .await
            .ok()
    }

    /// Stops the engine. The next call starts it again.
    pub async fn stop(&self) {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let Some(conn) = self.inner.conn.lock().await.take() else {
            return;
        };
        if conn.alive.load(Ordering::SeqCst) {
            let _ = conn
                .call::<()>(self.inner.id(), Call::Shutdown, SHUTDOWN_GRACE)
                .await;
            let deadline = Instant::now() + SHUTDOWN_GRACE;
            while conn.alive.load(Ordering::SeqCst) && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            if conn.alive.load(Ordering::SeqCst) {
                conn.kill();
            }
        }
    }

    /// Kills the engine as if it had crashed. For testing recovery.
    #[doc(hidden)]
    pub async fn crash_for_test(&self) {
        if let Some(conn) = self.inner.current().await {
            conn.kill();
        }
    }
}

/// A dictation in progress on the engine. Dropping it without calling
/// [`Session::finish`] or [`Session::cancel`] cancels it.
pub struct Session {
    inner: Arc<Inner>,
    conn: Arc<Conn>,
    id: u64,
    done: bool,
}

impl Session {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Streams audio to the engine. Never blocks.
    pub fn feed(&self, samples: &[f32]) -> Result<(), EngineError> {
        if samples.is_empty() {
            return Ok(());
        }
        self.conn
            .notify(Call::Feed(FeedParams::from_samples(self.id, samples)))
    }

    /// A handle that can cancel this session while `finish` is waiting.
    pub fn canceller(&self) -> Canceller {
        Canceller {
            conn: self.conn.clone(),
            id: self.id,
        }
    }

    pub async fn finish(mut self, timeout: Duration) -> Result<TranscribeResult, EngineError> {
        self.done = true;
        let call = Call::Finish(SessionParams { session: self.id });
        let result = self.conn.call(self.inner.id(), call, timeout).await;
        if matches!(result, Err(EngineError::Timeout)) {
            self.canceller().cancel();
        }
        result
    }

    pub async fn cancel(mut self) -> Result<(), EngineError> {
        self.done = true;
        let call = Call::Cancel(SessionParams { session: self.id });
        self.conn.call(self.inner.id(), call, CALL_TIMEOUT).await
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.done {
            self.canceller().cancel();
        }
    }
}

#[derive(Clone)]
pub struct Canceller {
    conn: Arc<Conn>,
    id: u64,
}

impl Canceller {
    /// Drops the session and aborts its transcription. Fire and forget.
    pub fn cancel(&self) {
        let _ = self
            .conn
            .notify(Call::Cancel(SessionParams { session: self.id }));
    }
}

impl Inner {
    fn id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn current(&self) -> Option<Arc<Conn>> {
        let slot = self.conn.lock().await;
        slot.as_ref()
            .filter(|c| c.alive.load(Ordering::SeqCst))
            .cloned()
    }

    /// The running engine, started if needed.
    async fn connection(self: &Arc<Self>) -> Result<Arc<Conn>, EngineError> {
        let mut slot = self.conn.lock().await;
        if let Some(conn) = slot.as_ref().filter(|c| c.alive.load(Ordering::SeqCst)) {
            return Ok(conn.clone());
        }
        self.stopping.store(false, Ordering::SeqCst);
        let conn = self.spawn().await?;
        *slot = Some(conn.clone());
        Ok(conn)
    }

    async fn spawn(self: &Arc<Self>) -> Result<Arc<Conn>, EngineError> {
        check_cpu()?;
        let mut variant = *self.variant.lock().unwrap();
        let exe = match engine_exe(variant) {
            Ok(exe) => exe,
            Err(e) if variant == Variant::Vulkan => {
                self.fall_back_to_cpu("The GPU engine isn't installed.".into());
                tracing::warn!("{e}");
                variant = Variant::Cpu;
                engine_exe(variant)?
            }
            Err(e) => return Err(e),
        };

        let mut cmd = Command::new(&exe);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd
            .spawn()
            .map_err(|e| EngineError::Spawn(format!("{}: {e}", exe.display())))?;
        if let Some(job) = &self.job {
            job.assign(&child);
        }
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let tail = Arc::new(StdMutex::new(VecDeque::new()));
        tokio::spawn(read_stderr(stderr, tail.clone()));

        let mut stdout = BufReader::new(stdout);
        let info = match tokio::time::timeout(READY_TIMEOUT, wait_ready(&mut stdout)).await {
            Ok(Some(info)) => info,
            Ok(None) => {
                let code = child.wait().await.ok().and_then(|s| s.code());
                if code.is_some_and(|c| c as u32 == STATUS_ILLEGAL_INSTRUCTION) {
                    return Err(EngineError::CpuUnsupported);
                }
                let last = tail.lock().unwrap().back().cloned();
                return Err(EngineError::Spawn(last.unwrap_or_else(|| {
                    format!("it exited during start-up (code {code:?})")
                })));
            }
            Err(_) => {
                let _ = child.start_kill();
                return Err(EngineError::Timeout);
            }
        };
        if info.protocol != PROTOCOL_VERSION {
            let _ = child.start_kill();
            return Err(EngineError::Incompatible(info.protocol));
        }

        let (lines, queued) = mpsc::unbounded_channel();
        tokio::spawn(write_stdin(stdin, queued));
        let conn = Arc::new(Conn {
            lines,
            pending: StdMutex::new(HashMap::new()),
            info: info.clone(),
            variant,
            alive: AtomicBool::new(true),
            loading: StdMutex::new(None),
            stderr: tail,
            child: StdMutex::new(Some(child)),
        });
        tokio::spawn(read_stdout(self.clone(), conn.clone(), stdout));
        tracing::info!(version = %info.version, ?variant, exe = %exe.display(), "engine started");

        self.replay(&conn).await;
        let _ = self.events.send(EngineEvent::Started { variant, info });
        Ok(conn)
    }

    async fn replay(&self, conn: &Conn) {
        let replay = self.replay.lock().unwrap().clone();
        if let Some(params) = replay.configure
            && let Err(e) = conn
                .call::<()>(self.id(), Call::Configure(params), CALL_TIMEOUT)
                .await
        {
            tracing::warn!("couldn't re-apply engine settings: {e}");
        }
        for params in replay.models {
            let id = params.model.id.clone();
            match conn.load(self.id(), params).await {
                Ok(loaded) => {
                    tracing::info!(model = %id, device = ?loaded.device, "reloaded model")
                }
                Err(e) => {
                    tracing::warn!(model = %id, "couldn't reload model: {e}");
                    self.replay
                        .lock()
                        .unwrap()
                        .models
                        .retain(|m| m.model.id != id);
                }
            }
        }
    }

    fn fall_back_to_cpu(&self, reason: String) {
        *self.variant.lock().unwrap() = Variant::Cpu;
        tracing::warn!("falling back to the CPU engine: {reason}");
        let _ = self.events.send(EngineEvent::FellBackToCpu { reason });
    }

    /// Called when an engine process ends, expectedly or not.
    fn exited(self: &Arc<Self>, conn: &Conn, code: Option<i32>) {
        if self.stopping.load(Ordering::SeqCst) {
            tracing::info!(?code, "engine stopped");
            return;
        }
        let illegal = code.is_some_and(|c| c as u32 == STATUS_ILLEGAL_INSTRUCTION);
        let message = if illegal {
            "The processor can't run this engine build.".to_owned()
        } else {
            crash_reason(&conn.stderr.lock().unwrap()).unwrap_or_else(|| match code {
                Some(code) => format!("exit code {:#x}", code as u32),
                None => "no exit code".to_owned(),
            })
        };
        tracing::warn!(?code, variant = ?conn.variant, "engine exited unexpectedly: {message}");

        let crashes = {
            let mut crashes = self.crashes.lock().unwrap();
            let now = Instant::now();
            crashes.retain(|t| now.duration_since(*t) < CRASH_WINDOW);
            crashes.push_back(now);
            crashes.len()
        };
        let loading = conn.loading.lock().unwrap().take();
        if conn.variant == Variant::Vulkan && (loading.is_some() || crashes >= 2 || illegal) {
            self.fall_back_to_cpu(message.clone());
        } else if let Some(model) = loading {
            // The CPU engine crashing on a load points at the model file.
            tracing::warn!(%model, "not reloading a model that crashed the engine");
            self.replay
                .lock()
                .unwrap()
                .models
                .retain(|m| m.model.id != model);
        }

        let restart_in = BACKOFF.get(crashes - 1).copied();
        let _ = self.events.send(EngineEvent::Crashed {
            message,
            restart_in,
        });
        if let Some(delay) = restart_in {
            let inner = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                if inner.stopping.load(Ordering::SeqCst) {
                    return;
                }
                if let Err(e) = inner.connection().await {
                    tracing::warn!("couldn't restart the engine: {e}");
                }
            });
        }
    }
}

impl Conn {
    async fn call<T: DeserializeOwned>(
        &self,
        id: u64,
        call: Call,
        timeout: Duration,
    ) -> Result<T, EngineError> {
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        // Checked after registering, so a crash in between can't strand the call.
        if !self.alive.load(Ordering::SeqCst)
            || self.lines.send(Request::call(id, call).to_line()).is_err()
        {
            self.pending.lock().unwrap().remove(&id);
            return Err(EngineError::Stopped);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(response)) => response.into_result().map_err(EngineError::Rpc),
            Ok(Err(_)) => Err(EngineError::Stopped),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(EngineError::Timeout)
            }
        }
    }

    async fn load(&self, id: u64, params: LoadParams) -> Result<LoadResult, EngineError> {
        *self.loading.lock().unwrap() = Some(params.model.id.clone());
        let result = self.call(id, Call::Load(params), LOAD_TIMEOUT).await;
        if self.alive.load(Ordering::SeqCst) {
            *self.loading.lock().unwrap() = None;
        }
        result
    }

    fn notify(&self, call: Call) -> Result<(), EngineError> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(EngineError::Stopped);
        }
        self.lines
            .send(Request::notify(call).to_line())
            .map_err(|_| EngineError::Stopped)
    }

    fn kill(&self) {
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.start_kill();
        }
    }

    fn dispatch(&self, line: &str) {
        match Outgoing::parse(line) {
            Ok(Outgoing::Response(response)) => {
                let waiter = self.pending.lock().unwrap().remove(&response.id);
                match waiter {
                    Some(tx) => {
                        let _ = tx.send(response);
                    }
                    None => tracing::debug!(id = response.id, "dropped a late engine response"),
                }
            }
            Ok(Outgoing::Event(Event::Log(log))) => log_engine(&log),
            Ok(Outgoing::Event(Event::Ready(_))) => {}
            Err(_) => tracing::debug!(target: "joona::engine", "{line}"),
        }
    }
}

/// The engine's own crash report or the last error it printed.
fn crash_reason(stderr: &VecDeque<String>) -> Option<String> {
    let mut lines = stderr.iter().rev();
    lines
        .clone()
        .find(|l| l.starts_with("joona-engine crashed"))
        .or_else(|| lines.find(|l| l.to_ascii_lowercase().contains("error")))
        .cloned()
}

fn check_cpu() -> Result<(), EngineError> {
    #[cfg(target_arch = "x86_64")]
    if !(std::arch::is_x86_feature_detected!("avx2")
        && std::arch::is_x86_feature_detected!("fma")
        && std::arch::is_x86_feature_detected!("f16c"))
    {
        return Err(EngineError::CpuUnsupported);
    }
    Ok(())
}

/// Installed builds sit next to `joona.exe`; the GPU build can also be a
/// download in the runtimes folder. `JOONA_ENGINE` overrides both.
fn engine_exe(variant: Variant) -> Result<PathBuf, EngineError> {
    if let Some(path) = std::env::var_os("JOONA_ENGINE") {
        return Ok(PathBuf::from(path));
    }
    let name = variant.binary_name();
    let mut places = Vec::new();
    if variant == Variant::Vulkan {
        places.push(crate::paths::runtimes_dir().join(&name));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        places.push(dir.join(&name));
        // Examples and tests run from a subfolder of the build directory.
        if let Some(up) = dir.parent() {
            places.push(up.join(&name));
        }
    }
    places
        .into_iter()
        .find(|p| p.is_file())
        .ok_or(EngineError::Missing(name))
}

/// Reads one line, tolerating text that isn't UTF-8.
async fn read_line(
    reader: &mut (impl AsyncBufReadExt + Unpin),
    buf: &mut Vec<u8>,
) -> Option<String> {
    buf.clear();
    match reader.read_until(b'\n', buf).await {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(String::from_utf8_lossy(buf).trim_end().to_owned()),
    }
}

async fn wait_ready(stdout: &mut (impl AsyncBufReadExt + Unpin)) -> Option<EngineInfo> {
    let mut buf = Vec::new();
    while let Some(line) = read_line(stdout, &mut buf).await {
        match Outgoing::parse(&line) {
            Ok(Outgoing::Event(Event::Ready(info))) => return Some(info),
            Ok(Outgoing::Event(Event::Log(log))) => log_engine(&log),
            _ => tracing::debug!(target: "joona::engine", "{line}"),
        }
    }
    None
}

async fn read_stdout(inner: Arc<Inner>, conn: Arc<Conn>, mut stdout: impl AsyncBufReadExt + Unpin) {
    let mut buf = Vec::new();
    while let Some(line) = read_line(&mut stdout, &mut buf).await {
        if !line.is_empty() {
            conn.dispatch(&line);
        }
    }
    conn.alive.store(false, Ordering::SeqCst);
    conn.pending.lock().unwrap().clear();
    let child = conn.child.lock().unwrap().take();
    let code = match child {
        Some(mut child) => child.wait().await.ok().and_then(|s| s.code()),
        None => None,
    };
    inner.exited(&conn, code);
}

async fn read_stderr(stderr: impl AsyncRead + Unpin, tail: Arc<StdMutex<VecDeque<String>>>) {
    let mut stderr = BufReader::new(stderr);
    let mut buf = Vec::new();
    while let Some(line) = read_line(&mut stderr, &mut buf).await {
        if line.is_empty() {
            continue;
        }
        tracing::debug!(target: "joona::engine", "{line}");
        let mut tail = tail.lock().unwrap();
        if tail.len() == STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

async fn write_stdin(mut stdin: ChildStdin, mut lines: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = lines.recv().await {
        if stdin.write_all(line.as_bytes()).await.is_err() {
            break;
        }
    }
}

fn log_engine(log: &LogEvent) {
    match log.level {
        LogLevel::Debug => tracing::debug!(target: "joona::engine", "{}", log.message),
        LogLevel::Info => tracing::info!(target: "joona::engine", "{}", log.message),
        LogLevel::Warn => tracing::warn!(target: "joona::engine", "{}", log.message),
        LogLevel::Error => tracing::error!(target: "joona::engine", "{}", log.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_timeout_grows_with_the_audio() {
        assert_eq!(finish_timeout(0), Duration::from_secs(30));
        assert_eq!(finish_timeout(60_000), Duration::from_secs(210));
    }

    #[test]
    fn binary_names_follow_the_platform() {
        let cpu = Variant::Cpu.binary_name();
        assert!(cpu.starts_with("joona-engine"));
        assert!(cpu.ends_with(std::env::consts::EXE_SUFFIX));
        assert_ne!(cpu, Variant::Vulkan.binary_name());
    }

    #[test]
    fn crash_reason_prefers_the_engines_report() {
        let tail: VecDeque<String> = [
            "ggml_vulkan: error allocating buffer",
            "joona-engine crashed: panicked at main.rs:10",
            "whisper_init_state: compute buffer (decode) = 95.91 MB",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            crash_reason(&tail).as_deref(),
            Some("joona-engine crashed: panicked at main.rs:10")
        );

        let tail: VecDeque<String> = ["loading", "ggml_vulkan: Error allocating buffer", "done"]
            .map(String::from)
            .into();
        assert_eq!(
            crash_reason(&tail).as_deref(),
            Some("ggml_vulkan: Error allocating buffer")
        );
        assert_eq!(crash_reason(&VecDeque::from(["fine".to_owned()])), None);
    }

    #[tokio::test]
    async fn lines_that_are_not_utf8_are_still_read() {
        let data: &[u8] = b"ok\n\xff\xfe broken\nlast";
        let mut reader = BufReader::new(data);
        let mut buf = Vec::new();
        assert_eq!(
            read_line(&mut reader, &mut buf).await.as_deref(),
            Some("ok")
        );
        assert!(
            read_line(&mut reader, &mut buf)
                .await
                .unwrap()
                .ends_with("broken")
        );
        assert_eq!(
            read_line(&mut reader, &mut buf).await.as_deref(),
            Some("last")
        );
        assert_eq!(read_line(&mut reader, &mut buf).await, None);
    }
}
