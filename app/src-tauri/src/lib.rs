//! Desktop shell scaffold. No commands, plugins, or networking are registered.

/// Run the local placeholder window using the configured capabilities and CSP.
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    #[test]
    fn component_versions_match() {
        let version = meshyard_core::version();
        assert_eq!(version, meshyard_store::version());
        assert_eq!(version, meshyard_net::version());
        assert_eq!(version, meshyard_agents::version());
    }
}
