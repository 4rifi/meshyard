//! Command functions are shared by IPC handlers and real non-GUI integration checks.
use crate::{state::AppState, view::*};

pub async fn app_status(state: &AppState) -> AppStatus {
    let guard = state.0.lock().await;
    match guard.as_ref() {
        Ok(core) => core.status().await.unwrap_or_else(AppStatus::Error),
        Err(error) => AppStatus::Error(error.clone()),
    }
}
pub async fn create_workspace(state: &AppState, input: CreateWorkspace) -> AppStatus {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.create(input).await.unwrap_or_else(AppStatus::Error),
        Err(error) => AppStatus::Error(error.clone()),
    }
}
pub async fn update_profile(state: &AppState, name: String) -> AppStatus {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.profile(name).await.unwrap_or_else(AppStatus::Error),
        Err(error) => AppStatus::Error(error.clone()),
    }
}
pub async fn register_agent(state: &AppState, input: RegisterAgent) -> AppStatus {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.register(input).await.unwrap_or_else(AppStatus::Error),
        Err(error) => AppStatus::Error(error.clone()),
    }
}
pub(crate) mod ipc {
    use super::*;
    #[tauri::command]
    pub async fn app_status(state: tauri::State<'_, AppState>) -> Result<AppStatus, AppError> {
        Ok(super::app_status(&state).await)
    }
    #[tauri::command]
    pub async fn create_workspace(
        state: tauri::State<'_, AppState>,
        input: CreateWorkspace,
    ) -> Result<AppStatus, AppError> {
        Ok(super::create_workspace(&state, input).await)
    }
    #[tauri::command]
    pub async fn update_profile(
        state: tauri::State<'_, AppState>,
        name: String,
    ) -> Result<AppStatus, AppError> {
        Ok(super::update_profile(&state, name).await)
    }
    #[tauri::command]
    pub async fn register_agent(
        state: tauri::State<'_, AppState>,
        input: RegisterAgent,
    ) -> Result<AppStatus, AppError> {
        Ok(super::register_agent(&state, input).await)
    }
}
