# Meshyard — Product Specification

> "Your agents. Your machines. Your data."

Status: draft v1 (2026-11-02), owner: Dion Arifi. Planning: Claude Code. Implementation: OpenAI Codex.

## 1. Problem

Developers increasingly run several local coding agents (Claude Code, Codex CLI, …) at once, often
across a small team. Today there is no good way to see, across machines, what each agent is doing,
hand work between agents, or coordinate a team of humans plus agents — without uploading code,
prompts and histories to someone else's server.

## 2. Product

Meshyard is a desktop app (macOS first, Windows/Linux later) that:

1. Connects teammates' devices directly over the internet with end-to-end encrypted, mutually
   authenticated peer-to-peer links — no shared Wi-Fi, no port forwarding, no central app backend.
2. Observes locally running coding agents through their official integration points (hooks),
   never by guessing.
3. Shows every connected agent as an animated pixel-art avatar in a shared, live "yard". Avatars
   walk to stations (Build, Review, Test, Fix, Merge, Waiting, Finished) according to real events.
4. Later: lets agents hand tasks and selected context to each other, with explicit human approval.

Non-goals: a chatbot, a hosted SaaS dashboard, a cloud IDE, remote shell access.

## 3. Personas

- **Solo power user** running Claude Code + Codex in parallel on one or two machines.
- **Small team (2–5 devs)** in different places, each with their own agents, wanting shared
  awareness and handoffs.

## 4. Core user journey (target)

1. Install Meshyard.
2. Create a workspace (local only; generates device identity).
3. Connect local agents (Meshyard shows exactly which hook config it will add; user approves).
4. Invite a teammate via link or QR code.
5. Teammate opens the link → devices pair → both confirm a short verification code.
6. (Later) AI setup assistant proposes roles/stations/rules → user reviews the validated config.
7. Both enter the shared playground and see each other's agents live.

## 5. Functional requirements

### 5.1 Networking
- Direct P2P over the internet with NAT traversal; relay fallback only where disclosed.
- Two modes, chosen per workspace, visible at all times:
  - **P2P Auto** — direct first; end-to-end encrypted relay fallback allowed and labelled.
  - **Strict P2P** — no relay at all; if no direct path is possible, fail with a clear diagnosis.
- Mutual authentication by device key; only paired devices may connect.
- 2+ participants (MVP: 2; architecture: small full mesh, ≤ 8).
- Automatic reconnection with backoff; visible per-peer status (connecting / direct / relayed /
  offline) and diagnostics (path, RTT, last error).
- No silent downgrade to unencrypted or undisclosed transport — ever.
- We do not claim P2P works on every network; we report honestly when it doesn't.

### 5.2 Agent integration
- Initial adapters: **Claude Code** (hooks) and **OpenAI Codex CLI** (hooks).
- Adapter interface is extensible; capabilities differ per agent and are declared, not assumed.
- Activity states: Idle, Thinking, Building, Reviewing, Testing, Fixing, WaitingForInput, Merging,
  Finished, Error.
- Every state carries its provenance: **verified** (directly from an agent event, e.g. `Stop`,
  `PermissionRequest`) or **inferred** (derived by rule, e.g. `Bash` running `cargo test` ⇒
  Testing), plus the evidence rule id.
- Remote execution is out of scope for MVP; later only with explicit, scoped authorization.

### 5.3 Playground
- Shared pixel-art yard rendered with PixiJS; avatars move between stations on real events.
- Avatars show owner (user + device), agent kind, current state, provenance badge.
- Click avatar → inspector: state, current task summary (if shared), recent actions, timestamps.
- Connection/sync status always visible. Calm, coherent visual style; no clutter.
- Customization (post-MVP): layout, station set, avatar skins — driven by validated config.

### 5.4 AI-configured workspaces (post-MVP)
- User describes workflow in natural language; assistant emits a **declarative, schema-validated
  config** (roles, stations, workflows, rules, permissions, layout). Never executable code.
- User reviews a diff and approves before anything applies.

### 5.5 Privacy & sharing
- Project files, prompts, credentials, transcripts stay local by default.
- Per-workspace sharing policy controls what leaves the device (default: state + tool category +
  coarse task label; never prompt text, file contents, command lines, or secrets).
- Clear statement of what signaling/relay infrastructure can observe (see `SECURITY.md`).
- Meshyard's guarantees are distinct from third-party model providers' processing.

### 5.6 Collaboration & Git (post-MVP)
- Task assignment and agent-to-agent handoffs; PR review workflows; branch awareness;
  per-agent git worktrees; conflict surfacing; human approval for merges / destructive ops.

## 6. MVP definition (Milestone 1)

Done when, on two Macs on **different networks**:

1. Both run Meshyard; one creates a workspace and invites the other.
2. Pairing via invite link/QR + verification code yields an authenticated, encrypted connection.
3. Real messages flow both ways (agent state events + presence).
4. A shared playground shows both users' agents.
5. At least Claude Code is integrated via real hooks; Codex adapter is a stretch goal for M1.
6. Agent state changes on one machine move the avatar on both machines within ~1 s (direct path).
7. Diagnostics show mode, path type (direct/relay), RTT, and actionable errors.
8. Direct and relay-assisted connections are tested separately; Strict P2P failure is reported
   correctly when no direct path exists.

Hardware tests that were not actually performed are reported as "not yet validated", never as passed.

## 7. Quality bar

Modular, typed interfaces, secure defaults, structured logs without secrets, automated tests for
every completed feature, no fake functionality, no unnecessary dependencies.
