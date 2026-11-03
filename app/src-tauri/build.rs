fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "app_status",
            "create_workspace",
            "update_profile",
            "register_agent",
        ]),
    ))
    .unwrap_or_else(|error| panic!("app build failed: {error}"));
}
