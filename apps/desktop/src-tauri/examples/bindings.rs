//! Writes `src/lib/bindings.ts` without starting the app. Debug builds of the
//! app also do this at start-up.

fn main() -> anyhow::Result<()> {
    let path = joona_lib::write_bindings()?;
    println!("wrote {}", path.display());
    Ok(())
}
