//! Local ingest and bounded correlation; app transport/registration belongs to T-007b.
pub mod ingest;
pub mod state;

pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
