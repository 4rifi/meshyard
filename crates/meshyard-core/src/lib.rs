//! Pure domain, frozen wire v1, signatures and deterministic projection.
pub mod crypto;
pub mod domain;
pub mod fold;
pub mod pairing;
pub mod policy;
pub mod wire;
pub use crypto::*;
pub use domain::*;
pub use fold::*;
pub use policy::*;

// Kept for the T-001 application scaffold until its bridge is replaced in T-009.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
