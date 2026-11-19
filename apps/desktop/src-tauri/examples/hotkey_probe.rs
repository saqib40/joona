//! Installs the dictation hotkey hook and prints every gesture action.
//! `cargo run -p joona --example hotkey_probe -- 20` runs for 20 seconds.

use std::time::{Duration, Instant};

use joona_lib::hotkeys::{GestureConfig, Hotkeys};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "joona_lib::hotkeys=trace".into()),
        )
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .init();
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _hotkeys = Hotkeys::start(GestureConfig::default(), tx)?;
    println!("hook installed, listening for {secs}s");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        while let Ok(action) = rx.try_recv() {
            println!("{:>6} ms  {action:?}", start.elapsed().as_millis());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
