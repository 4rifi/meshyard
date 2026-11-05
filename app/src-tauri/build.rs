fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "network_status",
            "network_start",
            "network_stop",
            "network_invite",
            "network_join",
            "network_confirm",
            "app_status",
            "create_workspace",
            "update_profile",
            "register_agent",
            "bind_agent",
            "hook_plan",
            "hook_apply",
            "hook_remove",
        ]),
    ))
    .unwrap_or_else(|error| panic!("app build failed: {error}"));
}
