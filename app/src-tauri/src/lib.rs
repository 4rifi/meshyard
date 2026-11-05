//! Local workspace application. No network endpoint, hook, filesystem or shell plugin.
pub mod commands;
pub mod hooks;
mod ingest;
pub mod network;
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
            tauri::async_runtime::block_on(state.attach_app(app.handle().clone()));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ipc::network_status,
            commands::ipc::network_start,
            commands::ipc::network_stop,
            commands::ipc::network_invite,
            commands::ipc::network_join,
            commands::ipc::network_confirm,
            commands::ipc::app_status,
            commands::ipc::create_workspace,
            commands::ipc::update_profile,
            commands::ipc::register_agent,
            commands::ipc::bind_agent,
            commands::ipc::hook_plan,
            commands::ipc::hook_apply,
            commands::ipc::hook_remove,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                tauri::async_runtime::block_on(app.state::<state::AppState>().shutdown());
            }
        });
    Ok(())
}
