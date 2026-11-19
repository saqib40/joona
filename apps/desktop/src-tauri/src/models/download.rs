//! Installs models from the catalog. Downloads resume after a dropped
//! connection or a restart, every file is checked against its SHA-256, and
//! each mirror is tried in turn. Partial files live in `.downloads` inside the
//! models folder, so they never look like models.

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail, ensure};
use futures_util::StreamExt;
use reqwest::StatusCode;
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use tokio::io::AsyncWriteExt;

use super::catalog::{Catalog, CatalogFile, CatalogModel, Format, Source};
use super::library::{Library, MANIFEST, VAD_DIR};

const TRIES_PER_MIRROR: u32 = 3;
const REPORT_EVERY: Duration = Duration::from_millis(150);
const SPEED_WINDOW: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Downloading,
    Verifying,
    Converting,
    Unpacking,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub stage: Stage,
    /// Bytes so far. While converting, thousandths done.
    pub done: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
    pub error: Option<String>,
}

pub trait Listener: Send + Sync + 'static {
    fn progress(&self, progress: &Progress);
    /// A model finished installing.
    fn installed(&self, id: &str);
}

#[derive(Debug, thiserror::Error)]
#[error("the download was cancelled")]
pub struct Cancelled;

struct Job {
    cancel: Arc<AtomicBool>,
    progress: Progress,
}

pub struct Downloads {
    library: Library,
    catalog: &'static Catalog,
    client: reqwest::Client,
    jobs: Mutex<HashMap<String, Job>>,
    listener: Box<dyn Listener>,
}

impl Downloads {
    pub fn new(
        library: Library,
        catalog: &'static Catalog,
        listener: impl Listener,
    ) -> anyhow::Result<Arc<Self>> {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "Joona/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/saqib40/joona)"
            ))
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(45))
            .build()?;
        Ok(Arc::new(Self {
            library,
            catalog,
            client,
            jobs: Mutex::new(HashMap::new()),
            listener: Box::new(listener),
        }))
    }

    /// Starts installing a model in the background. Progress goes to the listener.
    pub fn start(self: &Arc<Self>, id: &str) -> anyhow::Result<()> {
        let model = self
            .catalog
            .get(id)
            .with_context(|| format!("{id} isn't in the catalog"))?
            .clone();
        let size = model.download_size();
        let Some(cancel) = self.register(id, size) else {
            return Ok(());
        };
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut report = Reporter::new(&this, &model.id);
            let result = this.install(&model, &cancel, &mut report).await;
            this.conclude(&mut report, &model.id, size, result);
        });
        Ok(())
    }

    /// Downloads `files` as one job, each to its path, then runs `unpack`.
    /// A file that's already there and intact isn't downloaded again.
    pub fn start_files(
        self: &Arc<Self>,
        id: &str,
        files: Vec<(CatalogFile, PathBuf)>,
        unpack: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let size = files.iter().map(|(file, _)| file.size).sum();
        let Some(cancel) = self.register(id, size) else {
            return Ok(());
        };
        let this = self.clone();
        let id = id.to_owned();
        tauri::async_runtime::spawn(async move {
            let mut report = Reporter::new(&this, &id);
            let result = async {
                let mut offset = 0;
                for (file, dest) in &files {
                    if let Some(dir) = dest.parent() {
                        tokio::fs::create_dir_all(dir).await?;
                    }
                    if verify(dest, file.size, &file.sha256).await.is_err() {
                        report.span = Some((offset, size));
                        this.fetch_file(file, dest, &cancel, Some(&mut report))
                            .await?;
                    }
                    offset += file.size;
                }
                report.span = None;
                report.set(Stage::Unpacking, size, size);
                tokio::task::spawn_blocking(unpack).await?
            }
            .await;
            this.conclude(&mut report, &id, size, result);
        });
        Ok(())
    }

    /// Adds a job for `id` unless one is running, and returns its cancel flag.
    fn register(&self, id: &str, total: u64) -> Option<Arc<AtomicBool>> {
        let mut jobs = lock(&self.jobs);
        if jobs
            .get(id)
            .is_some_and(|job| is_running(job.progress.stage))
        {
            return None;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Progress {
            id: id.to_owned(),
            stage: Stage::Downloading,
            done: 0,
            total,
            bytes_per_sec: 0,
            error: None,
        };
        jobs.insert(
            id.to_owned(),
            Job {
                cancel: cancel.clone(),
                progress,
            },
        );
        Some(cancel)
    }

    fn conclude(&self, report: &mut Reporter<'_>, id: &str, size: u64, result: anyhow::Result<()>) {
        match result {
            Ok(()) => {
                tracing::info!(id, "download installed");
                report.finish(Stage::Done, None, size);
                self.listener.installed(id);
            }
            Err(e) if e.is::<Cancelled>() => {
                // Keep the partial file, so downloading again resumes it.
                report.finish(Stage::Cancelled, None, size);
            }
            Err(e) => {
                tracing::warn!(id, "couldn't install the download: {e:#}");
                report.finish(Stage::Failed, Some(format!("{e:#}")), size);
            }
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        lock(&self.jobs).get(id).is_some_and(|job| {
            job.cancel.store(true, Ordering::Relaxed);
            is_running(job.progress.stage)
        })
    }

    /// The latest progress of every download since the app started.
    pub fn progress(&self) -> Vec<Progress> {
        let mut all: Vec<Progress> = lock(&self.jobs)
            .values()
            .map(|job| job.progress.clone())
            .collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }

    /// Downloads Silero VAD if it's missing.
    pub async fn ensure_vad(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = self.library.vad() {
            return Ok(path);
        }
        let file = &self.catalog.vad;
        let dir = self.library.root().join(VAD_DIR);
        tokio::fs::create_dir_all(&dir).await?;
        let dest = dir.join(&file.path);
        self.fetch_file(file, &dest, &AtomicBool::new(false), None)
            .await?;
        tracing::info!("installed Silero VAD");
        Ok(dest)
    }

    async fn install(
        &self,
        model: &CatalogModel,
        cancel: &AtomicBool,
        report: &mut Reporter<'_>,
    ) -> anyhow::Result<()> {
        if let Err(e) = self.ensure_vad().await {
            tracing::warn!("couldn't download Silero VAD, so silence won't be trimmed: {e:#}");
        }
        let staging = self.library.work_dir("downloads").join(&model.id);
        tokio::fs::create_dir_all(&staging).await?;
        let mut failures = Vec::new();
        for source in &model.sources {
            match self
                .install_from(model, source, &staging, cancel, report)
                .await
            {
                Ok(()) => {
                    let _ = tokio::fs::remove_dir_all(&staging).await;
                    return Ok(());
                }
                Err(e) if e.is::<Cancelled>() => return Err(e),
                Err(e) => {
                    tracing::warn!(model = %model.id, url = %source.url, "download source failed: {e:#}");
                    failures.push(format!("{}: {e:#}", host(&source.url)));
                }
            }
        }
        bail!("every download source failed ({})", failures.join("; "))
    }

    async fn install_from(
        &self,
        model: &CatalogModel,
        source: &Source,
        staging: &Path,
        cancel: &AtomicBool,
        report: &mut Reporter<'_>,
    ) -> anyhow::Result<()> {
        let size = source.size(model);
        let sha256 = source
            .sha256(model)
            .context("the catalog has no hash for this source")?;
        let part = staging.join(format!("{}.part", file_name(&source.url)));
        self.fetch(&source.url, &part, size, cancel, Some(report))
            .await?;
        report.set(Stage::Verifying, size, size);
        if let Err(e) = verify(&part, size, sha256).await {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }

        let ready = staging.join("ready");
        let _ = tokio::fs::remove_dir_all(&ready).await;
        tokio::fs::create_dir_all(&ready).await?;
        let folder = match source.format {
            Format::File => {
                tokio::fs::rename(&part, ready.join(&model.path)).await?;
                ready
            }
            Format::OpenaiPt => {
                let assets = self.converter_assets(cancel).await?;
                let out = ready.join(&model.path);
                convert(&part, &assets, &out, report).await?;
                if let Some(expected) = &model.sha256
                    && let Err(e) = verify(&out, model.size, expected).await
                {
                    bail!("the converted model doesn't match the published one: {e:#}");
                }
                let _ = tokio::fs::remove_file(&part).await;
                ready
            }
            Format::TarGz => {
                report.set(Stage::Unpacking, size, size);
                let folder = unpack(&part, &ready).await?;
                for file in &model.contains {
                    ensure!(folder.join(file).is_file(), "the archive has no {file}");
                }
                let _ = tokio::fs::remove_file(&part).await;
                folder
            }
        };

        let manifest = serde_json::to_vec_pretty(&model.manifest(source))?;
        tokio::fs::write(folder.join(MANIFEST), manifest).await?;
        let target = self.library.root().join(&model.id);
        if tokio::fs::metadata(&target).await.is_ok() {
            let old = staging.join("old");
            let _ = tokio::fs::remove_dir_all(&old).await;
            tokio::fs::rename(&target, &old).await?;
        }
        tokio::fs::rename(&folder, &target).await?;
        Ok(())
    }

    /// The files the checkpoint converter reads, downloaded once.
    async fn converter_assets(&self, cancel: &AtomicBool) -> anyhow::Result<PathBuf> {
        let dir = self.library.work_dir("cache").join("whisper-assets");
        tokio::fs::create_dir_all(&dir).await?;
        for file in &self.catalog.converter {
            let dest = dir.join(&file.path);
            if verify(&dest, file.size, &file.sha256).await.is_ok() {
                continue;
            }
            self.fetch_file(file, &dest, cancel, None).await?;
        }
        Ok(dir)
    }

    /// Downloads a file from the first mirror that works.
    async fn fetch_file(
        &self,
        file: &CatalogFile,
        dest: &Path,
        cancel: &AtomicBool,
        mut report: Option<&mut Reporter<'_>>,
    ) -> anyhow::Result<()> {
        let part = dest.with_extension("part");
        let mut failures = Vec::new();
        for url in &file.urls {
            let fetched = self
                .fetch(url, &part, file.size, cancel, report.as_deref_mut())
                .await;
            match fetched {
                Ok(()) => match verify(&part, file.size, &file.sha256).await {
                    Ok(()) => {
                        tokio::fs::rename(&part, dest).await?;
                        return Ok(());
                    }
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&part).await;
                        failures.push(format!("{}: {e:#}", host(url)));
                    }
                },
                Err(e) if e.is::<Cancelled>() => return Err(e),
                Err(e) => failures.push(format!("{}: {e:#}", host(url))),
            }
        }
        bail!("couldn't download {} ({})", file.path, failures.join("; "))
    }

    /// Downloads `url` into `part`, resuming what's there, retrying a dropped connection.
    async fn fetch(
        &self,
        url: &str,
        part: &Path,
        size: u64,
        cancel: &AtomicBool,
        mut report: Option<&mut Reporter<'_>>,
    ) -> anyhow::Result<()> {
        let mut failures = 0;
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!(Cancelled);
            }
            let have = tokio::fs::metadata(part).await.map_or(0, |m| m.len());
            if have == size {
                return Ok(());
            }
            if have > size {
                tokio::fs::remove_file(part).await?;
                continue;
            }
            match self
                .fetch_once(url, part, have, size, cancel, report.as_deref_mut())
                .await
            {
                Ok(()) => failures = 0,
                Err(e) if e.is::<Cancelled>() => return Err(e),
                Err(e) => {
                    failures += 1;
                    if failures >= TRIES_PER_MIRROR {
                        return Err(e);
                    }
                    tracing::debug!(url, "download interrupted, retrying: {e:#}");
                    tokio::time::sleep(Duration::from_secs(2 * u64::from(failures))).await;
                }
            }
        }
    }

    async fn fetch_once(
        &self,
        url: &str,
        part: &Path,
        have: u64,
        size: u64,
        cancel: &AtomicBool,
        mut report: Option<&mut Reporter<'_>>,
    ) -> anyhow::Result<()> {
        let mut request = self.client.get(url);
        if have > 0 {
            request = request.header(RANGE, format!("bytes={have}-"));
        }
        let response = request.send().await?;
        let status = response.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            tokio::fs::remove_file(part).await?;
            bail!("the server can't resume this download");
        }
        ensure!(status.is_success(), "the server answered {status}");
        // A server that ignores the range sends the whole file again.
        let resume = status == StatusCode::PARTIAL_CONTENT;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resume)
            .truncate(!resume)
            .open(part)
            .await?;
        let mut done = if resume { have } else { 0 };
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await?;
                bail!(Cancelled);
            }
            let chunk = chunk?;
            done += chunk.len() as u64;
            ensure!(
                done <= size,
                "the server sent more than the catalog's {size} bytes"
            );
            file.write_all(&chunk).await?;
            if let Some(report) = report.as_deref_mut() {
                report.set(Stage::Downloading, done, size);
            }
        }
        file.flush().await?;
        ensure!(
            done == size,
            "the connection closed after {done} of {size} bytes"
        );
        Ok(())
    }
}

fn is_running(stage: Stage) -> bool {
    !matches!(stage, Stage::Done | Stage::Failed | Stage::Cancelled)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}

fn file_name(url: &str) -> String {
    let name = url.rsplit('/').next().unwrap_or("download");
    let name: String = name
        .split(['?', '#'])
        .next()
        .unwrap_or("download")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if name.is_empty() {
        "download".into()
    } else {
        name
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

async fn verify(path: &Path, size: u64, sha256: &str) -> anyhow::Result<()> {
    let (path, sha256) = (path.to_owned(), sha256.to_owned());
    tokio::task::spawn_blocking(move || {
        let len = std::fs::metadata(&path)?.len();
        ensure!(len == size, "the file is {len} bytes instead of {size}");
        let actual = sha256_file(&path)?;
        ensure!(
            actual == sha256,
            "the file is damaged (SHA-256 {actual} instead of {sha256})"
        );
        Ok(())
    })
    .await?
}

async fn convert(
    pt: &Path,
    assets: &Path,
    out: &Path,
    report: &mut Reporter<'_>,
) -> anyhow::Result<()> {
    let permille = Arc::new(AtomicU32::new(0));
    let mut task = tokio::task::spawn_blocking({
        let (pt, assets, out, permille) = (
            pt.to_owned(),
            assets.to_owned(),
            out.to_owned(),
            permille.clone(),
        );
        move || {
            super::convert::pt_to_ggml(&pt, &assets, &out, |done| {
                permille.store((done * 1000.0) as u32, Ordering::Relaxed);
            })
        }
    });
    loop {
        report.set(
            Stage::Converting,
            u64::from(permille.load(Ordering::Relaxed)),
            1000,
        );
        tokio::select! {
            result = &mut task => return result?,
            () = tokio::time::sleep(REPORT_EVERY) => {}
        }
    }
}

/// Unpacks a `.tar.gz` into `into`, returning the folder holding the files.
async fn unpack(archive: &Path, into: &Path) -> anyhow::Result<PathBuf> {
    let (archive, into) = (archive.to_owned(), into.to_owned());
    tokio::task::spawn_blocking(move || {
        let file = std::io::BufReader::new(std::fs::File::open(&archive)?);
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in tar.entries().context("the archive is damaged")? {
            let mut entry = entry.context("the archive is damaged")?;
            let path = entry.path()?.into_owned();
            // Archives made on a Mac carry metadata files nobody needs.
            let junk = path.components().any(|c| {
                let name = c.as_os_str().to_string_lossy();
                name.starts_with("._") || name == "__MACOSX" || name == ".DS_Store"
            });
            // `unpack_in` refuses entries that would land outside `into`.
            if !junk && !entry.unpack_in(&into)? {
                bail!("the archive tries to write outside its folder");
            }
        }
        Ok(single_folder(&into).unwrap_or(into))
    })
    .await?
}

/// Archives usually wrap their files in one folder.
fn single_folder(dir: &Path) -> Option<PathBuf> {
    let mut entries = std::fs::read_dir(dir).ok()?.filter_map(Result::ok);
    let only = entries.next()?;
    (entries.next().is_none() && only.path().is_dir()).then(|| only.path())
}

/// Throttles progress reports and measures the download speed.
struct Reporter<'a> {
    downloads: &'a Downloads,
    id: String,
    last: Option<(Stage, Instant)>,
    samples: VecDeque<(Instant, u64)>,
    /// While one of several files downloads: the bytes before it, and the
    /// bytes of them all.
    span: Option<(u64, u64)>,
}

impl<'a> Reporter<'a> {
    fn new(downloads: &'a Downloads, id: &str) -> Self {
        Self {
            downloads,
            id: id.to_owned(),
            last: None,
            samples: VecDeque::new(),
            span: None,
        }
    }

    fn set(&mut self, stage: Stage, done: u64, total: u64) {
        let (done, total) = match self.span {
            Some((before, all)) => (before + done, all),
            None => (done, total),
        };
        let now = Instant::now();
        if stage == Stage::Downloading
            && self
                .samples
                .back()
                .is_none_or(|(t, _)| now - *t >= Duration::from_millis(100))
        {
            self.samples.push_back((now, done));
            while self.samples.len() > 2
                && self
                    .samples
                    .front()
                    .is_some_and(|(t, _)| now - *t > SPEED_WINDOW)
            {
                self.samples.pop_front();
            }
        }
        if self
            .last
            .is_some_and(|(s, at)| s == stage && now - at < REPORT_EVERY)
        {
            return;
        }
        self.last = Some((stage, now));
        self.publish(stage, done, total, None);
    }

    fn finish(&mut self, stage: Stage, error: Option<String>, size: u64) {
        let (done, total) = match stage {
            Stage::Done => (size, size),
            _ => lock(&self.downloads.jobs)
                .get(&self.id)
                .map_or((0, size), |job| (job.progress.done, job.progress.total)),
        };
        self.publish(stage, done, total, error);
    }

    fn speed(&self) -> u64 {
        match (self.samples.front(), self.samples.back()) {
            (Some((t0, b0)), Some((t1, b1))) if t1 > t0 => {
                ((b1.saturating_sub(*b0)) as f64 / (*t1 - *t0).as_secs_f64()) as u64
            }
            _ => 0,
        }
    }

    fn publish(&self, stage: Stage, done: u64, total: u64, error: Option<String>) {
        let progress = Progress {
            id: self.id.clone(),
            stage,
            done,
            total,
            bytes_per_sec: if stage == Stage::Downloading {
                self.speed()
            } else {
                0
            },
            error,
        };
        if let Some(job) = lock(&self.downloads.jobs).get_mut(&self.id) {
            job.progress = progress.clone();
        }
        self.downloads.listener.progress(&progress);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_come_from_the_url() {
        assert_eq!(
            file_name("https://blob.handy.computer/ggml-small.bin"),
            "ggml-small.bin"
        );
        assert_eq!(
            file_name("https://x/a/model.tar.gz?download=true"),
            "model.tar.gz"
        );
        assert_eq!(file_name("https://x/"), "download");
        assert_eq!(host("https://huggingface.co/ggerganov/x"), "huggingface.co");
    }

    #[test]
    fn hashes_files() {
        let path = std::env::temp_dir().join(format!("joona-hash-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn finds_the_folder_an_archive_wraps_its_files_in() {
        let dir = std::env::temp_dir().join(format!("joona-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        assert_eq!(single_folder(&dir), Some(dir.join("inner")));
        std::fs::write(dir.join("loose.txt"), b"x").unwrap();
        assert_eq!(single_folder(&dir), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
