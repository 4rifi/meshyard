# KNOWER responses

## KQ-008 — Owner delivery and onboarding target (2026-11-03)

- **Requester:** Dion, direct product clarification.
- **Confirmed owner intent:** the desired end product is an installable macOS DMG. A user creates
  their own environment, shares an invitation link with a colleague, and the colleague connects
  through the app. Solo use must also work. After creating/joining, a prominent action such as
  `Join the Agent H+` should provide the next step.
- **Clarification pending:** whether `H+` means `HQ`, and whether this action opens the shared
  playground, connects an existing local agent, or starts a new agent session. KNOWER asked
  Dion; no exact action or final English label has been approved yet.
- **Fit with existing design:** PRODUCT §§4–6 already describe workspace creation, local agents,
  link/QR invitation, mutual verification and entry into the playground. PROTOCOL v1.1 §7 defines
  `meshyard://join/...` and pairing confirmation. Treat “set up a server” as a proposed user-facing
  `Create workspace` flow consistent with local-first P2P; it does not independently authorize a
  central backend or require the user to configure infrastructure.
- **Proposed UX, pending Claude specification:** DMG → install/open Meshyard → `Create workspace`
  or `Join with invite` → confirm pairing when joining → connect local agents → a clearly named
  HQ action. Solo workspace entry must not require a second device; display actual empty state
  when no agent is connected. The HQ action's meaning/order remains subject to Dion's answer.
- **Concrete planning gap:** inspected Tauri configuration currently targets only `app`; it is
  not a DMG release configuration. ROADMAP places signed/notarized packaging in M2, while T-011
  covers onboarding and T-012 system validation. Claude should schedule a DMG distribution task
  and installed-app/deep-link acceptance against this owner target without silently expanding
  current T-001/T-002 scope. External distribution still obeys G-1.
- **Release acceptance recommendation:** verify installation on two Macs, invitation opening from
  another app into installed Meshyard, both pairing confirmations, honest connection state,
  actual agent events and solo entry. DMG file existence alone proves none of these behaviors.
- **Scope:** product knowledge only. No app implementation, release/hosting action, task status,
  canonical product/spec rewrite, commit or push. This is a target, not an available capability.

## Canonical migration and knowledge refresh — 2026-11-03

- **Requester:** Dion, relaying Claude's canonical-checkout decision and copied knowledge files.
- **Result:** all five files present at `/Volumes/Kursmodule/meshyard/docs/knowledge`; recursive
  file-set and SHA-256 comparison proved the copied files unchanged before refresh. No missing
  or extra file. All 35 original local link targets existed.
- **Scope:** subsequent updates only in these five knowledge files; preserve original Documents
  copies, application code, HANDOFF, ROADMAP, task specs, Git staging and commit ownership.
- **Verified decisions:** Claude registered this session and recorded onboarding receipt.
  PROTOCOL v1.1/ADR-009/010 and T-002 now resolve KQ-001/KQ-004; knowledge status updated accordingly.
- **Milestone:** T-001 is review with passing full checks/build reported, while corrected native
  retest and original first-start latency acceptance remain open. The latest report explicitly
  retains the original latency requirement; no native permission or acceptance is inferred.
- **Evidence boundary:** canonical branch `t-001-scaffold`, inspected HEAD
  `c960af2eb81f20b69af4396cde37b5034a10a4e3`. Product checks are reported evidence, not KNOWER
  reruns. See [PROGRESS](PROGRESS.md) for current details and preserved historical observations.
- **Handoff:** after local reference/whitespace verification, these five files are ready for
  Hardworker's focused documentation commit under its current authorization. KNOWER does not
  stage, commit or push. HANDOFF's owner still needs to reconcile its older recovery-checkout path.
- **Delivery:** onboarding acknowledgement is already confirmed; migration commit-readiness
  notification is a separate follow-up and is not evidence of a commit or task acceptance.

## Historical initialization — 2026-11-02

- **Requester:** Dion, KNOWER onboarding assignment.
- **Status:** onboarding findings documented; architectural recommendations pending Claude review.
- **Recommendation:** finish and review T-001; resolve the relevant protocol questions before
  T-002 freezes wire types. Keep implementation with Hardworker and approval with Claude.
- **Evidence:** all current top-level project documents, task specs T-001…T-003, the independent
  architecture review, workspace manifests, crate entry points and frontend scaffold inspected.
  See [PROGRESS](PROGRESS.md) for the revision boundary and real command results.
- **Reasoning:** source currently implements scaffolding only. Accepted ADRs and normative
  protocol establish direction but do not prove runtime behavior or hardware acceptance.
- **Risks / unresolved:** [KQ-001…007](OPEN_QUESTIONS.md), particularly forwarding/invitation
  authority, exact wire contracts and ingest deadlines/evidence. Test tooling unavailable in
  this session; no passing check claimed.
- **Concrete guidance:** use PROTOCOL over old architecture summaries, do not widen sharing or
  Strict transport policy, and preserve offline tests plus the two-network field-test gates.
- **Files owned:** this `docs/knowledge/` directory only; no application or active shared planning
  files changed. No task status, acceptance verdict, new ADR, commit or push issued.
- **Delivery:** repository files available locally. Hardworker notification accepted by Codex
  queue without `--resume`, message `01a19e30-a56c-7433-ad28-acc2a455ea2d`. Recipient acknowledgement
  and Claude receipt are not established; shared HANDOFF remains untouched.

## Response template

```markdown
## KQ-XXX — Concise answer

- Requester / relevant revision:
- Status: answered advisory / awaiting architect / awaiting owner / needs evidence
- Recommended solution:
- Technical reasoning and alternatives:
- Relevant evidence (files/sections, commands/results, official sources if used):
- Risks and limits:
- Concrete implementation guidance and meaningful checks:
- Unresolved questions / decision owner:
- Canonical documentation update required:
- Delivery and acknowledgement evidence:
```

Update canonical specs through their owner after a decision. Preserve the distinction between
advice, accepted architecture, inspected source and executed validation.
