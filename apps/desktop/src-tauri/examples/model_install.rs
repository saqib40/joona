//! Installs a catalog model the way the Hub does, printing progress.
//! `--interrupt-after` cancels part-way and starts again, to check resuming.
//!
//! cargo run --example model_install -- <model id> [--interrupt-after <seconds>]

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use joona_lib::models::Library;
use joona_lib::models::catalog::catalog;
use joona_lib::models::download::{Downloads, Listener, Progress, Stage};
use tokio::sync::mpsc;

struct Console(mpsc::UnboundedSender<Progress>);

impl Listener for Console {
    fn progress(&self, progress: &Progress) {
        let _ = self.0.send(progress.clone());
    }

    fn installed(&self, id: &str) {
        println!("installed {id}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "joona_lib=debug".into()),
        )
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .init();
    tauri::async_runtime::set(tokio::runtime::Handle::current());

    let mut args = std::env::args().skip(1);
    let id = args
        .next()
        .context("usage: model_install <model id> [--interrupt-after <seconds>]")?;
    let interrupt = match args.next().as_deref() {
        Some("--interrupt-after") => Some(Duration::from_secs_f64(
            args.next().context("missing the seconds")?.parse()?,
        )),
        Some(other) => bail!("unknown option {other}"),
        None => None,
    };

    let library = Library::new(joona_lib::paths::models_dir());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let downloads = Downloads::new(library.clone(), catalog(), Console(tx))?;
    let started = Instant::now();
    downloads.start(&id)?;
    if let Some(after) = interrupt {
        let watch = tokio::spawn(async move {
            let last = report(&mut rx).await;
            (rx, last)
        });
        tokio::time::sleep(after).await;
        println!("cancelling: {}", downloads.cancel(&id));
        let (back, last) = watch.await?;
        rx = back;
        println!(
            "stopped at {:?} with {} bytes; starting again",
            last.stage, last.done
        );
        downloads.start(&id)?;
    }
    let last = report(&mut rx).await;
    println!(
        "{:?} after {:.1} s",
        last.stage,
        started.elapsed().as_secs_f64()
    );
    if let Some(error) = last.error {
        bail!(error);
    }
    let model = library
        .get(&id)
        .context("the model isn't in the library after installing")?;
    println!(
        "library has {} at {}",
        model.id(),
        model.spec().path.display()
    );
    Ok(())
}

/// Prints progress about once a second until the download stops.
async fn report(rx: &mut mpsc::UnboundedReceiver<Progress>) -> Progress {
    let mut printed: Option<(Stage, Instant)> = None;
    while let Some(p) = rx.recv().await {
        let finished = matches!(p.stage, Stage::Done | Stage::Failed | Stage::Cancelled);
        if finished
            || printed.is_none_or(|(stage, at)| {
                stage != p.stage || at.elapsed() >= Duration::from_secs(1)
            })
        {
            match p.stage {
                Stage::Converting => println!("Converting: {:.0}%", p.done as f64 / 10.0),
                _ => println!(
                    "{:?}: {:.1} of {:.1} MB, {:.1} MB/s",
                    p.stage,
                    p.done as f64 / 1e6,
                    p.total as f64 / 1e6,
                    p.bytes_per_sec as f64 / 1e6
                ),
            }
            printed = Some((p.stage, Instant::now()));
        }
        if finished {
            return p;
        }
    }
    panic!("the downloader went away");
}
