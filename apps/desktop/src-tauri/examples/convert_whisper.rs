//! Converts an OpenAI Whisper checkpoint to GGML.
//! `cargo run -p joona --example convert_whisper -- tiny.pt <assets-dir> ggml-tiny.bin`

use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let [pt, assets, out] = args.as_slice() else {
        anyhow::bail!("usage: convert_whisper <model.pt> <assets-dir> <out.bin>");
    };
    let started = Instant::now();
    let mut last = 0;
    joona_lib::models::convert::pt_to_ggml(pt, assets, out, |p| {
        let pct = (p * 100.0) as u32;
        if pct >= last + 25 {
            last = pct;
            println!("{pct}%");
        }
    })?;
    println!(
        "wrote {} ({} bytes) in {:.1}s",
        out.display(),
        std::fs::metadata(out)?.len(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
