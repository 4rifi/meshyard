//! Local workspace application. No network endpoint, hook, filesystem or shell plugin.
pub mod commands;
pub mod state;
pub mod view;
use tauri::Manager;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .setup(|app| {
            let dir = std::env::var_os("MESHYARD_DATA_DIR")
                .map(std::path::PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| app.path().app_data_dir());
            let file_key = std::env::var_os("MESHYARD_KEY_STORE");
            let state = match (dir, file_key.as_deref()) {
                (Ok(dir), None) => {
                    tauri::async_runtime::block_on(state::AppState::open(dir, false))
                }
                (Ok(dir), Some(key)) if key == "file" => {
                    tauri::async_runtime::block_on(state::AppState::open(dir, true))
                }
                _ => state::AppState::failed(view::AppError::Configuration),
            };
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ipc::app_status,
            commands::ipc::create_workspace,
            commands::ipc::update_profile,
            commands::ipc::register_agent,
        ])
        .run(tauri::generate_context!())
}
