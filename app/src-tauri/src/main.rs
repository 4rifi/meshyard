#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    match meshyard_app::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            tracing::error!("desktop shell stopped unexpectedly");
            std::process::ExitCode::FAILURE
        }
    }
}
