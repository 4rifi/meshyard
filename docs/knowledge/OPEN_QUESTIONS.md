# Open technical questions

Initial assessment: KNOWER, 2026-11-02; status refreshed 2026-11-03 in the canonical USB checkout.
Original problem statements below are historical where a resolution is recorded. Evidence is
inspected documentation; consequences and remedies are advisory inferences, not reproduced runtime defects.
Claude owns architectural resolution; Dion owns product/privacy/cost decisions. Do not silently
amend accepted guarantees or stop another agent's unrelated work based on this list.

## Current disposition — 2026-11-03

| ID | Status | Verified evidence / remaining work |
|---|---|---|
| KQ-001 | Resolved by architect | ADR-009; PROTOCOL v1.1 §§7–8: sender-only logs, owner-only invites, Welcome carries only signed genesis |
| KQ-002 | Open | PROTOCOL §9 still has 200 ms stdin versus ≤150 ms total failure budget; before T-007 |
| KQ-003 | Open | Ingest outcome evidence still needs an exact allowlist/fixture contract; before T-007/T-008 |
| KQ-004 | Resolved by architect | ADR-010; PROTOCOL v1.1 §§1–5 and T-002 specify exact signed bytes, closed v1, owner fold and validation stages |
| KQ-005 | Open | Workspace mode and endpoint lifecycle still need a decision before T-003 |
| KQ-006 | Partly addressed | Full scaffold checks now reported passing against pinned iroh; legacy summaries and actual networking APIs remain to validate |
| KQ-007 | Partly addressed | Session registered and onboarding acknowledged; license/hosting owner decisions remain open |
| KQ-008 | Owner target recorded; CTA clarification pending | Installable DMG, invitation onboarding and solo entry; Claude to schedule packaging/deep-link acceptance |

Architecture resolution is not implementation acceptance. Closed issues retain the original
evidence below for rationale; their replacement contract is the canonical PROTOCOL, not the
initial recommendation.

## KQ-001 — Author forwarding and invitation authority

**Relevant before:** T-002 wire freeze; T-005/T-006 behavior. **Decision owner:** Claude.

Original evidence (PROTOCOL v1, superseded by ADR-009): §2 permits only events authored by the authenticated connection peer. §8
describes catch-up over the union of author heads, potentially including stored third-party events.
§7 allows an inviter to write `MemberAdded`, while §4 authorizes it only for the genesis owner.

Recommendation: explicitly limit MVP sync transmission to the sender's own log and MVP invitations
to the genesis owner. Define validation of owner membership events in Welcome. Keep third-party
sync/inviter delegation in M2 unless Claude deliberately specifies another authorized mechanism.
Signed events alone do not override the current no-forwarding rule.

## KQ-002 — Hook deadlines conflict

**Relevant before:** T-007 specification. **Decision owner:** Claude.

Evidence: PROTOCOL §9 allows a 200 ms stdin deadline, 50 ms connect and 50 ms write, yet requires a
total failure-path budget ≤150 ms. ARCHITECTURE §7.1 still says missing socket exits within 50 ms.
An input timeout alone can exceed the stated total budget.

Recommendation: define a single end-to-end deadline and allocate stage budgets within it, or
revise the total target explicitly. Measure empty/malformed/slow stdin, absent socket and blocked
writes separately. KNOWER does not select a replacement SLA.

## KQ-003 — Ingest fields do not carry all state-machine evidence

**Relevant before:** T-007/T-008 fixtures and schema. **Decision owner:** Claude.

Evidence: PROTOCOL §9's field allowlist omits request status and tool outcome, while its Codex
error rule needs failed/successful `PostModelRequest` results. ARCHITECTURE §7.3 also distinguishes
failed tools and subsequent fixes. A message with only event name and IDs cannot carry every
stated distinction.

Recommendation: specify bounded local enums/booleans for outcomes actually available in captured
hook fixtures; exclude arbitrary output/error text. Validate with official version-specific
documentation and scrubbed payloads before promising a state. No live hook capture was performed.

## KQ-004 — Wire contracts need a few remaining exact definitions

**Relevant before:** T-002 golden bytes and schema freeze. **Decision owner:** Claude.

Original evidence (PROTOCOL v1, superseded by ADR-010): §1 uses `canonical(Genesis)` without explicitly naming its encoding or signing
domain; §5 freezes types after review but also mentions appending enum variants. §4's fold first
processes owner membership and then "every other author", leaving the owner’s ordinary agent/profile
events implicit. Signed strings are to have control characters stripped, with no explicit stage.

Recommendation: specify genesis bytes/signature input and v1 variant compatibility, seed owner
membership from genesis, and fold ordinary events for every member including the owner. Define
whether malformed remote strings are rejected or only sanitized in a view model; never mutate
received signed bytes before verification or equivocation comparison. Add golden and fold cases.
This identifies omissions, not evidence that the current stubs implement them incorrectly.

## KQ-005 — Mode lifetime and endpoint ownership

**Relevant before:** T-003/T-005 endpoint configuration and mode changes. **Decision owner:** Claude.

Evidence: workspace genesis includes mode and determines WorkspaceId (PROTOCOL §1); the inviter's
transcript includes mode (§7). ARCHITECTURE §5.1 permits mode changes by endpoint rebuild, and the
UI/domain model describes per-workspace modes. No explicit cross-workspace endpoint lifecycle or
owner authorization for changing genesis-bound mode is defined.

Recommendation: scope M1 to one active workspace/mode per process, with mode fixed for that
workspace's genesis, unless Claude defines a versioned change protocol. Rebinding alone must not
silently change the workspace identity contract or weaken another workspace's Strict guarantee.

## KQ-006 — Legacy summaries and verification scope

**Relevant before:** affected implementation tasks. **Decision owner:** Claude, with Hardworker evidence.

Evidence: ARCHITECTURE §§4–7 retain random WorkspaceId, short legacy framing, six-word SAS,
free-text provenance, `max_seq`, and short hook timing. PROTOCOL overrides those summaries.
The architecture review checked iroh 1.0.0; the scaffold manifest pins 1.3.0. ROADMAP R-6 still
describes self-hosted relays as an M2 option, whereas G-1 requires them before external distribution.

Recommendation: follow normative PROTOCOL now, reconcile summaries through the owning agent, and
verify exact pinned iroh APIs/test controls in T-003. Treat G-1 as mandatory irrespective of
milestone labeling. Reconcile ARCHITECTURE's absolute relay-storage claim with SECURITY §3's
operator-dependent limitation. No new library/API claim has been independently tested by KNOWER.

## KQ-007 — Owner decisions and coordination completion

**Relevant before:** distribution / publishing / next session. **Decision owners:** Dion and Claude.

Original evidence (registration subsequently completed; see current disposition): license D-5 is open; relay hosting/operator/cost details still need T-R1. HANDOFF D-4
contains broad commit/push authorization, while its active T-001 row explicitly prohibits worker
push and assigns publication to the parent. HANDOFF lists KNOWER's thread ID as pending.

Recommendation: keep KNOWER advisory and uncommitted; respect the active task's narrower Git
scope. Have HANDOFF's owner register the verified KNOWER ID from README and acknowledge the
knowledge handoff when read. Do not infer receipt from file creation, a session ID or queue exit.


## KQ-008 — DMG onboarding and post-setup HQ action

**Owner intent recorded:** installable macOS DMG, workspace creation or invitation-link joining,
solo use, and a prominent next action after setup. See the full dated
[owner clarification and planning guidance](KNOWER_RESPONSES.md#kq-008--owner-delivery-and-onboarding-target-2026-11-03).

**Still open:** what the suggested `Join the Agent H+` action actually does and its final name.
Dion has been asked; do not equate opening the playground, attaching a local agent and launching
an agent session. Claude owns the task/spec changes. Current build targets only `app`, so a DMG
and installed-app deep-link validation remain deliverables to schedule, not completed features.
