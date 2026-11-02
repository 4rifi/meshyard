//! Core crate scaffold. Domain behavior is introduced in later tasks.

/// The scaffold package version; performs no I/O.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_compatible() {
        let parts: Vec<_> = super::version().split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|part| part.parse::<u32>().is_ok()));
    }
}
