//! Run from the repository root: cargo run -p meshyard-core --example export_bindings
use meshyard_core::*;
use ts_rs::TS;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/ipc/generated");
    let config = ts_rs::Config::default().with_out_dir(out);
    Workspace::export_all(&config)?;
    Agent::export_all(&config)?;
    Ok(())
}
