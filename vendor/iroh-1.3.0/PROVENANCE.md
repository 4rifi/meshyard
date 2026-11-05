# iroh 1.3.0 provenance

- Source: https://static.crates.io/crates/iroh/iroh-1.3.0.crate
- Registry download: https://crates.io/api/v1/crates/iroh/1.3.0/download
- Archive SHA-256: `885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833`
- Verified against the original registry entry in Meshyard Cargo.lock at `5e1046a`.
- Upstream VCS revision: `0072d7d84b233f9e7185eb676f049beaf557ac03`, path `iroh`, preserved in `.cargo_vcs_info.json`.
- Original archive files, including `LICENSE-BSD3`, are retained. This is third-party dependency code, outside the seven-member Meshyard workspace.

## Patch

`../iroh-1.3.0.patch` records the TLS source delta against the published archive.
Apply `../iroh-1.3.0-strict.patch` afterwards for the separately authorized Strict release feature.
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

## Strict release feature (2026-11-05)

Architect authorization: `/Volumes/TokensAbuse/logs/architect-strict-build-1.log`.
`../iroh-1.3.0-strict.patch` adds `strict-only` in Cargo.toml and gates out the
production/staging service maps in `src/defaults.rs`, the Default/Staging relay
variants and default constructor in `src/endpoint.rs`, both N0 presets in
`src/endpoint/presets.rs`, and DNS/pkarr service modules/reexports in
`src/address_lookup.rs`. `src/test_utils.rs` also gates its removed DNS/pkarr
helpers, while preserving generic local relay test support. The generic DNS staggering schedule is moved from
`src/address_lookup/dns.rs` to the parent module because custom resolver/net-report
scheduling also uses it. No host value is changed or hidden. Generic custom
relay APIs and system DNS primitives remain for dependency compatibility; Meshyard
Strict configuration refuses any relay, uses no lookup, and its release binary
must pass the unchanged forbidden-host guard. Non-strict behavior is unchanged.

The published archive checksum and TLS patch/proof above remain authoritative.
Reapply the two patches in order to that exact archive; inspect both diffs and
repeat default tests, Strict direct/config tests, TLS proof and the release bundle
guard after any upstream bump. This feature is additive in Cargo and must be
explicitly enabled by each release root. No new vendored crate is introduced.

The maintained feature input lives in `Cargo.toml.orig`. The normalized upstream
`Cargo.toml` is generated, never the source of truth. Reproduce it deterministically:

```sh
python3 scripts/generate-iroh-strict-manifest.py --archive /Volumes/TokensAbuse/tmp/iroh-1.3.0.crate
python3 scripts/generate-iroh-strict-manifest.py --archive /Volumes/TokensAbuse/tmp/iroh-1.3.0.crate --check
```

The generator verifies the exact archive SHA-256 above, verifies that the source
manifest contains only the approved feature delta, then applies that feature to
the archive's pristine Cargo-normalized manifest. This avoids reconstructing the
upstream workspace merely to normalize one feature. The recorded initial manual
normalized edit was replaced by this generated output before commit.
