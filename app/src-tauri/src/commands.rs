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
    let result = {
        let mut guard = state.0.lock().await;
        match guard.as_mut() {
            Ok(core) => core.create(input).await.unwrap_or_else(AppStatus::Error),
            Err(error) => AppStatus::Error(error.clone()),
        }
    };
    state.start_ingest().await;
    if matches!(result, AppStatus::Ready(_)) {
        app_status(state).await
    } else {
        result
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
pub async fn bind_agent(state: &AppState, agent_id: String) -> AppStatus {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.bind(agent_id).await.unwrap_or_else(AppStatus::Error),
        Err(error) => AppStatus::Error(error.clone()),
    }
}
pub async fn hook_plan(state: &AppState) -> Result<HookPlan, AppError> {
    plan(state, false).await
}
pub async fn hook_remove(state: &AppState) -> Result<HookPlan, AppError> {
    plan(state, true).await
}
async fn plan(state: &AppState, remove: bool) -> Result<HookPlan, AppError> {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.hook_plan(remove),
        Err(error) => Err(error.clone()),
    }
}
pub async fn hook_plan_for(
    state: &AppState,
    remove: bool,
    kind: meshyard_core::AgentKind,
) -> Result<HookPlan, AppError> {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core.hook_plan_for(remove, kind),
        Err(error) => Err(error.clone()),
    }
}
pub async fn hook_apply(state: &AppState, plan_id: String) -> AppStatus {
    let mut guard = state.0.lock().await;
    match guard.as_mut() {
        Ok(core) => core
            .hook_apply(plan_id)
            .await
            .unwrap_or_else(AppStatus::Error),
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
    #[tauri::command]
    pub async fn bind_agent(
        state: tauri::State<'_, AppState>,
        agent_id: String,
    ) -> Result<AppStatus, AppError> {
        Ok(super::bind_agent(&state, agent_id).await)
    }
    #[tauri::command]
    pub async fn hook_plan(
        state: tauri::State<'_, AppState>,
        kind: Option<meshyard_core::AgentKind>,
    ) -> Result<HookPlan, AppError> {
        super::hook_plan_for(
            &state,
            false,
            kind.unwrap_or(meshyard_core::AgentKind::ClaudeCode),
        )
        .await
    }
    #[tauri::command]
    pub async fn hook_remove(
        state: tauri::State<'_, AppState>,
        kind: Option<meshyard_core::AgentKind>,
    ) -> Result<HookPlan, AppError> {
        super::hook_plan_for(
            &state,
            true,
            kind.unwrap_or(meshyard_core::AgentKind::ClaudeCode),
        )
        .await
    }
    #[tauri::command]
    pub async fn hook_apply(
        state: tauri::State<'_, AppState>,
        plan_id: String,
    ) -> Result<AppStatus, AppError> {
        Ok(super::hook_apply(&state, plan_id).await)
    }
}
