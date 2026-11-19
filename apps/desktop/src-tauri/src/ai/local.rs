//! Joona's own AI model: a small chat model that llama.cpp's server runs on
//! this PC. The server starts when a dictation needs it, listens only on
//! 127.0.0.1 with a key made up for each start, and stops after a while
//! unused.

use std::collections::VecDeque;
use std::hash::{BuildHasher, RandomState};
use std::io::BufReader as StdBufReader;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use regex::Regex;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::models::catalog::{CatalogFile, LocalAiFiles, catalog};
use crate::providers::http_for;
use crate::supervisor::job::KillOnClose;

/// What its download is called.
pub const DOWNLOAD_ID: &str = "local-ai";
const SERVER_DIR: &str = "server";
const SERVER_EXE: &str = "llama-server.exe";
/// Loading the model from a slow disk can take a while.
const START_TIMEOUT: Duration = Duration::from_secs(90);
const HEALTH_EVERY: Duration = Duration::from_millis(250);
/// Room for the prompt, a long dictation and the answer, in tokens.
const CONTEXT: u32 = 4096;
/// Lines of the server's output kept to explain why it stopped.
const STDERR_TAIL: usize = 12;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Server {
    child: Child,
    base: String,
    key: String,
}

pub struct LocalAi {
    dir: PathBuf,
    server: Mutex<Option<Server>>,
    last_used: StdMutex<Instant>,
    job: Option<KillOnClose>,
}

impl LocalAi {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            // The server runs in its own folder, where relative paths break.
            dir: std::path::absolute(&dir).unwrap_or(dir),
            server: Mutex::new(None),
            last_used: StdMutex::new(Instant::now()),
            job: KillOnClose::new(),
        }
    }

    /// The model and server in the catalog.
    pub fn files() -> Option<&'static LocalAiFiles> {
        catalog().local_ai.as_ref()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The files to download, and where each goes.
    pub fn downloads(&self) -> Vec<(CatalogFile, PathBuf)> {
        Self::files()
            .map(|files| {
                [&files.model, &files.server]
                    .into_iter()
                    .map(|file| (file.clone(), self.dir.join(&file.path)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn installed(&self) -> bool {
        Self::files().is_some_and(|files| self.dir.join(&files.model.path).is_file())
            && server_exe(&self.dir).is_file()
    }

    /// The server is running or starting.
    pub fn running(&self) -> bool {
        match self.server.try_lock() {
            Ok(mut server) => server
                .as_mut()
                .is_some_and(|s| matches!(s.child.try_wait(), Ok(None))),
            Err(_) => true,
        }
    }

    /// Starts the server unless it's running. Returns its address and key.
    pub async fn start(&self) -> anyhow::Result<(String, String)> {
        let mut server = self.server.lock().await;
        if let Some(running) = server.as_mut() {
            match running.child.try_wait() {
                Ok(None) => {
                    self.touch();
                    return Ok((running.base.clone(), running.key.clone()));
                }
                _ => {
                    tracing::warn!("the local AI server had stopped");
                    *server = None;
                }
            }
        }
        let started = self.spawn().await?;
        let found = (started.base.clone(), started.key.clone());
        *server = Some(started);
        self.touch();
        Ok(found)
    }

    /// Notes that the model was just used, so it isn't stopped as idle.
    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    pub async fn stop(&self) {
        if let Some(mut server) = self.server.lock().await.take() {
            tracing::info!("stopping the local AI server");
            let _ = server.child.kill().await;
        }
    }

    /// Stops the server when it hasn't been used for `idle`, to free its memory.
    pub fn stop_if_idle(&self, idle: Duration) {
        // Locked means it's starting or answering.
        let Ok(mut server) = self.server.try_lock() else {
            return;
        };
        if server.is_some() && self.last_used.lock().unwrap().elapsed() >= idle {
            tracing::info!("stopping the local AI server after {idle:?} unused");
            if let Some(mut stopped) = server.take() {
                let _ = stopped.child.start_kill();
            }
        }
    }

    /// Stops the server and deletes the model and server.
    pub async fn delete(&self) -> anyhow::Result<()> {
        self.stop().await;
        // Windows can take a moment to let go of a stopped program's files.
        let mut tries = 0;
        loop {
            match tokio::fs::remove_dir_all(&self.dir).await {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) if tries < 10 => {
                    tries += 1;
                    tracing::debug!("couldn't delete the local AI yet: {e}");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                Err(e) => return Err(e).context("couldn't delete the local AI model"),
            }
        }
    }

    async fn spawn(&self) -> anyhow::Result<Server> {
        let files = Self::files().context("Joona's catalog has no local AI model")?;
        let model = self.dir.join(&files.model.path);
        let exe = server_exe(&self.dir);
        ensure!(
            model.is_file() && exe.is_file(),
            "the local AI model isn't downloaded"
        );
        let port = free_port().context("couldn't find a free port")?;
        let key = random_key();
        let mut cmd = Command::new(&exe);
        cmd.arg("--model")
            .arg(&model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--ctx-size", &CONTEXT.to_string(), "--parallel", "1"])
            .args(["--api-key", &key])
            .current_dir(self.dir.join(SERVER_DIR))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("couldn't start {}", exe.display()))?;
        if let Some(job) = &self.job {
            job.assign(&child);
        }
        let tail = Arc::new(StdMutex::new(VecDeque::new()));
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(read_output(stderr, tail.clone()));
        }

        let base = format!("http://127.0.0.1:{port}");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                // Give the last of its output a moment to arrive.
                tokio::time::sleep(Duration::from_millis(100)).await;
                match reason(&tail.lock().unwrap()) {
                    Some(why) => bail!("the local AI server stopped ({status}): {why}"),
                    None => bail!("the local AI server stopped ({status})"),
                }
            }
            if healthy(&base).await {
                break;
            }
            if started.elapsed() > START_TIMEOUT {
                let _ = child.kill().await;
                bail!("the local AI server took too long to start");
            }
            tokio::time::sleep(HEALTH_EVERY).await;
        }
        tracing::info!(port, "local AI server started in {:?}", started.elapsed());
        Ok(Server {
            child,
            base: format!("{base}/v1"),
            key,
        })
    }
}

fn server_exe(dir: &Path) -> PathBuf {
    dir.join(SERVER_DIR).join(SERVER_EXE)
}

/// Takes the server, its DLLs and licence out of llama.cpp's release zip in
/// `dir`, then deletes the zip.
pub fn unpack_server(dir: &Path) -> anyhow::Result<()> {
    let files = LocalAi::files().context("Joona's catalog has no local AI model")?;
    let zip = dir.join(&files.server.path);
    let staging = dir.join(format!("{SERVER_DIR}.new"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    let reader = StdBufReader::new(std::fs::File::open(&zip)?);
    let mut archive = zip::ZipArchive::new(reader).context("the server download is damaged")?;
    let mut found = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .context("the server download is damaged")?;
        if entry.is_dir() {
            continue;
        }
        // Everything goes in one folder, wherever the zip keeps it.
        let Some(name) = entry
            .enclosed_name()
            .and_then(|path| path.file_name().map(ToOwned::to_owned))
        else {
            continue;
        };
        let lower = name.to_string_lossy().to_ascii_lowercase();
        if lower != SERVER_EXE && !lower.ends_with(".dll") && !lower.starts_with("license") {
            continue;
        }
        found |= lower == SERVER_EXE;
        let mut out = std::fs::File::create(staging.join(&name))?;
        std::io::copy(&mut entry, &mut out)?;
    }
    if !found {
        let _ = std::fs::remove_dir_all(&staging);
        bail!("the server download doesn't have {SERVER_EXE} in it");
    }

    let target = dir.join(SERVER_DIR);
    if target.exists() {
        std::fs::remove_dir_all(&target).context("couldn't replace the old server")?;
    }
    std::fs::rename(&staging, &target)?;
    let _ = std::fs::remove_file(&zip);
    Ok(())
}

async fn healthy(base: &str) -> bool {
    let url = format!("{base}/health");
    let probe = http_for(&url)
        .get(&url)
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    probe.is_ok_and(|response| response.status().is_success())
}

/// The first error among the server's last lines, which is nearest the
/// cause, without llama.cpp's timestamp.
fn reason(tail: &VecDeque<String>) -> Option<String> {
    static STAMP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\d.]+ [A-Z] +").unwrap());
    let line = tail
        .iter()
        .find(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("error") || line.contains("failed")
        })
        .or_else(|| tail.back())?;
    Some(STAMP.replace(line, "").trim().to_owned())
}

fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

/// 128 random bits as hex, so other programs on the PC can't use the server.
fn random_key() -> String {
    let state = RandomState::new();
    (0..2u8)
        .map(|n| format!("{:016x}", state.hash_one((n, Instant::now()))))
        .collect()
}

async fn read_output(output: impl AsyncRead + Unpin, tail: Arc<StdMutex<VecDeque<String>>>) {
    let mut output = BufReader::new(output);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match output.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf).trim().to_owned();
        if line.is_empty() {
            continue;
        }
        tracing::trace!(target: "llama_server", "{line}");
        let mut tail = tail.lock().unwrap();
        if tail.len() == STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::SimpleFileOptions;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("joona-local-ai-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn the_server_and_its_dlls_come_out_of_the_zip() {
        let dir = scratch("unpack");
        let zip = dir.join(&LocalAi::files().unwrap().server.path);
        write_zip(
            &zip,
            &[
                ("build/bin/llama-server.exe", b"exe"),
                ("build/bin/ggml-cpu-haswell.dll", b"dll"),
                ("build/bin/llama-cli.exe", b"other"),
                ("LICENSE", b"MIT"),
                ("../escape.dll", b"bad"),
            ],
        );
        unpack_server(&dir).unwrap();
        let server = dir.join(SERVER_DIR);
        assert_eq!(std::fs::read(server.join(SERVER_EXE)).unwrap(), b"exe");
        assert!(server.join("ggml-cpu-haswell.dll").is_file());
        assert!(server.join("LICENSE").is_file());
        assert!(!server.join("llama-cli.exe").exists());
        assert!(!dir.join("escape.dll").exists());
        assert!(!zip.exists(), "the zip is deleted");

        let local = LocalAi::new(dir.clone());
        assert!(!local.installed(), "the model isn't there yet");
        std::fs::write(dir.join(&LocalAi::files().unwrap().model.path), b"gguf").unwrap();
        assert!(local.installed());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_zip_without_the_server_is_refused() {
        let dir = scratch("empty");
        write_zip(
            &dir.join(&LocalAi::files().unwrap().server.path),
            &[("llama-cli.exe", b"other")],
        );
        assert!(unpack_server(&dir).is_err());
        assert!(!dir.join(SERVER_DIR).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_first_error_explains_a_stop() {
        let tail: VecDeque<String> = [
            "0.00.050.001 I main: loading model",
            "0.00.052.300 E gguf_init_from_file: failed to open GGUF file 'q.gguf'",
            "0.00.053.443 E srv  server_run_i: exiting due to model loading error",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            reason(&tail).as_deref(),
            Some("gguf_init_from_file: failed to open GGUF file 'q.gguf'")
        );
        assert_eq!(reason(&VecDeque::new()), None);
    }

    #[test]
    fn keys_are_long_and_differ() {
        let (a, b) = (random_key(), random_key());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
