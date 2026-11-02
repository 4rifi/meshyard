# T-003 — Network endpoint, modes, diagnostics

Status: ready (after T-002) · Owner: Codex · Reviewer: Claude

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
- Mode switch = close endpoint and rebind; never mutate mode in place.
- Error type `NetError` with user-facing advice strings (e.g. Strict + unreachable).

## Tests (two separate suites, both required)
- **direct suite**: two in-process endpoints, relay disabled, connected via explicit direct
  addresses; assert path is IP, ping works, non-allowlisted peer is rejected without the handler
  reading data.
- **relay suite**: local relay from iroh's `test-utils` (or `iroh-relay` dev server); Auto-mode
  endpoints connected via relay-only address; assert diagnostics report `Relayed` at least
  initially, and data flows.
- **strict suite**: Strict-mode endpoint has no relay configured (assert on config/home relay
  = none); connecting to a peer known only by relay URL fails with the Strict advice error.
- Tests must not depend on public internet; mark any optional internet test `#[ignore]`.

## Acceptance criteria
- [ ] All three suites pass locally, deterministic over 10 consecutive runs (report the loop result).
- [ ] No app frame processed from non-allowlisted peers (test proves it).
- [ ] `npm run check` green.
- [ ] Report in `docs/reports/T-003.md`, including which iroh APIs differed from the architecture
      doc (and update ARCHITECTURE.md accordingly, marked "updated by Codex, pending review").
