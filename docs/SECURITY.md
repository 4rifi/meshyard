# Meshyard — Security & Privacy Model

Status: v1 (2026-11-02). Any change that weakens a guarantee here needs the owner's approval.

## 1. Guarantees (what we promise)

1. Application data between Meshyard peers is always end-to-end encrypted and mutually
   authenticated (QUIC + TLS 1.3 with raw Ed25519 public keys, via iroh).
2. Only devices that completed pairing for a workspace get application frames decoded or processed
   on `sync/1`/`ping/1`, and only for workspaces they were paired into (PROTOCOL §6).
3. In **Strict P2P**, the endpoint has no relay configured — project/agent data cannot traverse
   relay infrastructure. If no direct path exists, the connection fails and the UI says so.
4. In **P2P Auto**, relaying is possible, is always visible per peer ("Relayed via <host>"), and
   relayed bytes remain end-to-end encrypted.
5. There is no Meshyard application server. Repositories, prompts, transcripts, credentials and
   file contents are never uploaded by Meshyard.
6. Meshyard never silently falls back to unencrypted or undisclosed transport.

## 2. Non-guarantees (what we must not claim)

- P2P connectivity on every network. Symmetric NATs/firewalls can prevent direct paths.
- Anything about how Anthropic, OpenAI or other model providers process data the agents send to
  them. That is governed by those providers' policies, independent of Meshyard.
- Protection against a compromised local machine or a malicious paired teammate (they see what
  the sharing policy shares).

## 3. Infrastructure visibility (control plane)

| Component | Operator (default) | Sees | Never sees |
|---|---|---|---|
| DNS/pkarr address lookup | n0 (`iroh.link`) — **Auto only**; Strict uses none (ADR-011) | EndpointId (public key); home relay URL (Auto) | workspace names, members, any app data |
| Router port mapping (Strict and Auto) | your own router (UPnP/NAT-PMP/PCP) | the UDP port Meshyard asks to open | app data |
| Relay (Auto only) | n0 public relays (default) or self-hosted | EndpointIds of both sides, IP addresses, timing and volume of encrypted packets | plaintext of any stream |
| STUN-like / QAD probes | relays | your public IP:port | app data |

Relay policy (owner decision, 2026-11-02):
- n0 public relays are used **only for the development prototype with non-sensitive data**.
- Before Meshyard is distributed to external users, Auto mode switches to **self-hosted iroh
  relays in Switzerland/EU** (`RelayMode::Custom`), at least two for redundancy, with minimal
  logging. This is release gate **G-1** in `ROADMAP.md`.
- Our relay configuration is designed without persistent storage; relays only forward encrypted
  packets. For public operators (n0) storage/logging is governed by their policies, not by us. In Auto, the first packets of a connection
  may travel via the relay before the path upgrades to direct; the UI always shows the actual
  path ("Direct P2P" / "Encrypted relay").
- Self-hosting pkarr/DNS discovery as well removes n0 from the control plane entirely (M2).
- **Strict P2P contacts no n0 service** (ADR-011): no relay, no DNS/pkarr. Peers learn each
  other's addresses only from the invite ticket and the authenticated connection. The invite
  link therefore contains the inviter's IP addresses; it is shared only with the invitee.
- **Tester builds** (owner decision 2026-11-03): a build may go to a named test partner before
  G-1 only if it is Strict-only (no relay URL, no n0 DNS host compiled in, Auto disabled) and a
  build check proves it.

The settings screen shows which relay operator is in use.

## 3a. NAT traversal limitations

- Direct paths need successful hole punching or one reachable side. Symmetric NATs, strict
  corporate firewalls, or UDP-blocking networks can prevent them.
- Auto mode then stays on the encrypted relay. Strict mode fails and explains why.
- Meshyard logs path changes with peer short-ids only, no IP addresses at default log level.

## 4. Threat model (MVP)

| Threat | Mitigation |
|---|---|
| Network attacker reads/modifies traffic | QUIC/TLS 1.3, keys pinned to EndpointId |
| Stranger connects to my device | ALPN + allowlist check before any app frame is processed |
| Leaked invite link | single use, 15 min expiry, token stored as hash, atomic reservation to one joiner, transcript-bound **SAS confirmation** on both sides; a link can't weaken local Strict mode silently |
| Peer forges events for others / fake membership | signed events, owner key anchored in WorkspaceId, per-body authorization, author must equal connection peer (MVP) |
| Malicious peer sends huge/garbage frames | max frame size, bounded batches, schema validation, per-peer rate limits, version check |
| Other OS user injects fake agent events | Unix socket in `0700` dir, `0600`, peer-UID check. **Does not** protect against other processes of the same user — documented limitation |
| Secrets leak into logs or shared events | allowlisted field extraction in `meshyard-hook`; `SharingPolicy` filter; `tracing` redaction layer; tests asserting no prompt/command text in shared payloads |
| Device key theft from disk | key in macOS Keychain; fallback file `0600`; never in SQLite, logs, or IPC |
| Hook installer damages agent config | show exact diff, explicit approval, backup, idempotent, uninstall path |
| Malicious AI-generated workspace config (M3) | declarative schema only, strict validation, no code, diff + approval |
| Remote task execution abuse (M4) | off by default, per-peer per-action grants, human approval, worktree sandboxing |
| Tauri webview compromise | strict CSP, no remote content, minimal Tauri capabilities per window, no shell plugin |

## 5. Sharing policy (defaults)

Shared by default: agent kind, label, activity state, provenance, tool **category** (edit / shell /
read / web / other), coarse task label **only if the user typed one in Meshyard**.

Never shared by default: prompt text, assistant messages, file paths, file contents, command
lines, environment variables, transcript paths, tool outputs, git diffs.

Users can widen sharing per workspace later (M2) with an explicit, per-field opt-in.

## 6. Logging rules

- Structured logs only (`tracing`). Fields with names matching `token|secret|key|prompt|command|
  content|password|ticket` are redacted by a layer; tests enforce it.
- Invite tickets and secret keys must never reach `Debug` output (custom `Debug` impls).
- Log files stay local, rotate, and are excluded from any future crash reporting unless opted in.

## 7. Responsible behavior of the product

Meshyard never installs software, edits agent config, opens network modes, or grants permissions
without explicit user action in the UI.
