# iroh 1.3.0 provenance

- Source: https://static.crates.io/crates/iroh/iroh-1.3.0.crate
- Registry download: https://crates.io/api/v1/crates/iroh/1.3.0/download
- Archive SHA-256: `885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833`
- Verified against the original registry entry in Meshyard Cargo.lock at `5e1046a`.
- Upstream VCS revision: `0072d7d84b233f9e7185eb676f049beaf557ac03`, path `iroh`, preserved in `.cargo_vcs_info.json`.
- Original archive files, including `LICENSE-BSD3`, are retained. This is third-party dependency code, outside the seven-member Meshyard workspace.

## Patch

`../iroh-1.3.0.patch` is the complete source delta against the published archive.
Its only production changes in `src/tls.rs` are:

1. Client `enable_early_data`: `true` to `false`.
2. Server `max_early_data_size`: `u32::MAX` to `0`.

The additional `#[cfg(test)]` assertions inspect the actual built rustls client/server objects immediately before conversion to the opaque QUIC configs. The `tls::tests::built_configs_disable_early_data` unit test invokes both builders using an ephemeral key. No production refactor, certificate verification, identity handling or QUIC setting was changed. Upstream unsafe code is untouched.

The archive's own Cargo.lock is retained unless Cargo must update it for the explicit standalone vendor test. Any Cargo-generated change and test dependency availability are recorded in `docs/reports/T-003.md` in the Meshyard repository; lockfiles are never hand edited.

## Verification

From the Meshyard worktree root after sourcing its USB environment:

```sh
cargo test --manifest-path vendor/iroh-1.3.0/Cargo.toml --lib --offline --locked --no-default-features --features tls-ring tls::
```

Only TLS-filtered library tests are executed. No upstream Internet test is selected. Workspace tests do not discover these tests; the explicit command is required. Remove this dependency patch once upstream provides a supported early-data switch, after equivalent review and proofs.
