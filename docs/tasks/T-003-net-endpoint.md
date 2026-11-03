# T-003 — Network endpoint, modes, diagnostics

Status: ready (after T-002) · Owner: Codex · Reviewer: Claude
Revised 2026-11-02 after Codex review. Normative: `docs/PROTOCOL.md` §5–6 (framing, admission, limits).

## Objective
A `MeshNode` in `meshyard-net` that binds an iroh endpoint in P2P Auto or Strict P2P mode,
routes ALPNs, enforces the member allowlist, and reports honest path diagnostics.

## Scope
- `MeshNode::bind(config: NetConfig) -> Result<MeshNode>` where
  `NetConfig { secret_key, mode: Mode, relay: RelayChoice {N0Default, Custom(RelayMap)}, bind_addr: Option<SocketAddr> }`.
  - Auto → `presets::N0` (or custom relay map). Strict → `presets::N0DisableRelay`.
  - Verify the exact iroh 1.x API names against docs.rs for the pinned version; ARCHITECTURE.md
    names (`Connection::paths()`, `paths_stream()`, `Path::is_ip()/is_relay()`) come from
    research on 2026-11-02 and must be confirmed.
- ALPN router for `meshyard/pair/1`, `meshyard/sync/1`, `meshyard/ping/1`. `sync/1` and `ping/1`
  handlers reject connections whose remote EndpointId is not in the allowlist **before reading any
  frame**; `pair/1` only accepted while an invite is open (stub the invite check behind a trait
  for now; real pairing is T-005).
- `ping/1`: request/response with timestamps; returns RTT and path snapshot.
- `PeerDiagnostics { endpoint_id, path: Direct|Relayed{relay_url}|None, rtt, last_error, last_seen }`
  and `NodeDiagnostics { mode, home_relay, direct_addrs, peers }`, updated from `paths_stream()`.
  `path` is derived from the **selected** path only (`is_selected() && is_ip()` → Direct), copied
  out of the borrowed snapshot. Semantics: local send path, not a guarantee about every received byte.
- Mode switch = close endpoint and rebind; never mutate mode in place. One active workspace and
  one endpoint per process (KQ-005); the mode comes from that workspace's genesis.
- **Strict = no n0 services (ADR-011, 2026-11-03):** do not use `presets::N0DisableRelay` if it
  still enables n0 DNS/pkarr; build the Strict endpoint with `RelayMode::Disabled` and **no
  address lookup/publishing**. Verify the exact builder API in pinned iroh 1.3.0 and report it.
- **Strict builder (KQ-009, verified API):** `Endpoint::builder(presets::Minimal)` +
  `.relay_mode(RelayMode::Disabled)` + `.clear_address_lookup()` + `NetReportConfig::minimal()`.
  Never `N0DisableRelay`. Do not await `endpoint.online()` in Strict (relay-only readiness).
  Test asserts `endpoint.address_lookup()?.is_empty()`.
- **Port mapping (KQ-009 decision, ADR-012):** Meshyard owns one mapper: direct dependency
  `portmapper = "=0.19.3"` (same upstream code iroh uses), iroh's internal mapper disabled. A small
  `meshyard-net/src/portmap.rs` sets the client's UDP local port from the bound IPv4 socket,
  watches `watch_external_address()`, and calls `Endpoint::add_external_addr` /
  `remove_external_addr` on change, expiry, network change and shutdown. Diagnostics:
  `port_mapping: Disabled | Probing | Mapped { external } | NotObserved { after_secs }` —
  `NotObserved` is an observation ("no mapping within 10 s"), never stated as router refusal.
  Behind a trait so tests inject mapping events; no test touches a real router.
- `NodeDiagnostics.direct_addrs` must be complete enough to build an invite ticket for Strict.
- Relay configuration is a build/config input, never a hardcoded default in Strict code paths;
  a `strict-only` cargo feature (or equivalent config switch) makes Auto unavailable for tester
  builds (G-1 addendum). Implement the switch now; packaging uses it in T-013.
- Error type `NetError` with user-facing advice strings that separate **observation** ("no direct
  path within 10 s") from **suspected cause** ("likely: both peers behind NAT without port mapping").
- Two-stage admission per PROTOCOL §6 (device admission before decoding any frame; workspace
  admission via `Hello` is wired in T-006). Disable 0-RTT. Per-device connection/stream limits.
- Test endpoints MUST be built without public discovery (no `presets::N0`/staging in tests: build
  the endpoint manually with no address lookup, explicit local relay map, explicit addresses).

## Tests (two separate suites, both required)
- **direct suite**: two in-process endpoints, relay disabled, connected via explicit direct
  addresses; assert path is IP, ping works, non-allowlisted peer is rejected without the handler
  reading data.
- **relay suite**: local relay from iroh's `test-utils` (or `iroh-relay` dev server); endpoints
  connected via relay-only address **with direct IP paths prevented in a controlled way** (e.g.
  iroh test transport utilities / IP transports disabled on one side — find the 1.0 mechanism and
  document it); assert selected path is relay and data flows. If no reliable mechanism exists,
  stop and report instead of writing a flaky test.
- **strict suite**: Strict-mode endpoint has no relay configured (assert on config/home relay
  = none) and no address lookup service (assert on builder/config; no DNS/pkarr task); connecting to a peer known only by relay URL fails with the Strict advice error, with
  address caches and other lookup sources excluded.
- Tests must not depend on public internet; mark any optional internet test `#[ignore]`.

## Acceptance criteria
- [ ] All three suites pass locally, deterministic over 10 consecutive runs (report the loop result).
- [ ] No app frame processed from non-allowlisted peers (test proves it).
- [ ] `npm run check` green.
- [ ] Report in `docs/reports/T-003.md`, including which iroh APIs differed from the architecture
      doc (and update ARCHITECTURE.md accordingly, marked "updated by Codex, pending review").
