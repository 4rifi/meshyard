//! App-owned DTO export avoids a core -> app dependency cycle.
use meshyard_app::view::*;
use ts_rs::TS;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/generated");
    let config = ts_rs::Config::default().with_out_dir(out);
    meshyard_app::network::NetworkView::export_all(&config)?;
    AppStatus::export_all(&config)?;
    CreateWorkspace::export_all(&config)?;
    RegisterAgent::export_all(&config)?;
    HookPlan::export_all(&config)?;
    Ok(())
}
