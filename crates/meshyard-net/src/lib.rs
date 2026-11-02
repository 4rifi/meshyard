//! Networking scaffold: iroh types only, without constructing an endpoint.

pub use iroh::EndpointId;

/// The scaffold package version; performs no I/O or network initialization.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_invalid_endpoint_ids_without_networking() {
        assert!("invalid endpoint".parse::<super::EndpointId>().is_err());
        assert_eq!(super::version(), meshyard_core::version());
    }
}
