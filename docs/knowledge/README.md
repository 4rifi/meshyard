# Meshyard knowledge index

Maintainer: KNOWER, technical advisor and project knowledge guardian. Initialized 2026-11-02.
This directory indexes evidence and unanswered questions; it does not replace architecture,
security policy, task specifications, or Claude's review authority.

## Canonical checkout and migration (2026-11-03)

Owner-designated canonical checkout: `/Volumes/Kursmodule/meshyard`. All future KNOWER writes
belong only to `docs/knowledge/` there. The incomplete Documents checkout is historical and
must not be repaired or used for current implementation. Build/toolchain/cache data remains
separate at `/Volumes/Kursmodule/Meshyard-build`; the earlier temporary recovery checkout is
historical. HANDOFF still names that recovery checkout in its current-state paragraph; the
owner's newer canonical-path instruction takes precedence, and HANDOFF's owner should reconcile it.

Before any refresh, KNOWER compared the old and new knowledge directory recursively: exactly
five files on each side, no missing or extra files, every SHA-256 identical. All 35 local Markdown
link targets existed in the new checkout. The refresh below preserves the dated onboarding
history while recording subsequent decisions; post-refresh files intentionally differ from
the original copy. Inspection base: `c960af2eb81f20b69af4396cde37b5034a10a4e3`,
`t-001-scaffold`; only this directory was untracked.

## Read first

1. [HANDOFF](../HANDOFF.md): coordination, ownership, current task and owner decisions.
2. [PROGRESS](PROGRESS.md): inspected implementation, verification limits, next milestones.
3. [OPEN_QUESTIONS](OPEN_QUESTIONS.md): contradictions and recommendations pending review.

## Canonical sources

| Subject | Source | Interpretation |
|---|---|---|
| Vision, requirements, MVP acceptance | [PRODUCT](../PRODUCT.md) | Product specification, marked draft v1 |
| Components, stack and rationale | [ARCHITECTURE](../ARCHITECTURE.md) §§1–3, 8–11 | ADR-001…010 are recorded as accepted; §11 scaffold updates are pending review |
| Identity, signatures, authorization, wire, pairing, sync, ingest | [PROTOCOL](../PROTOCOL.md) | Normative over conflicting architecture summaries; currently v1.1 with ADR-009/010 clarifications |
| Threat model, metadata, privacy boundaries | [SECURITY](../SECURITY.md) | Weakening guarantees requires owner approval |
| Accepted decisions | [ARCHITECTURE](../ARCHITECTURE.md) §10 and [HANDOFF](../HANDOFF.md) §5 | Proposals here are not new accepted ADRs |
| Planned work and dependencies | [ROADMAP](../ROADMAP.md), [tasks](../tasks/) | Only Claude marks reviewed tasks done |
| Execution evidence and acceptance | `docs/reports/`, [reviews](../reviews/) | Inspect actual reports; source presence is not a successful run |
| Agent consultations | [QUESTIONS_FOR_KNOWER](QUESTIONS_FOR_KNOWER.md), [KNOWER_RESPONSES](KNOWER_RESPONSES.md) | Request/answer records with evidence and explicit delivery status |

Separate networking, security, stack, playground and decision documents are intentionally not
duplicated here. Follow the references above, then inspect the relevant code.

## Knowledge labels

- **Confirmed implementation:** inspected source, with file references; runtime success needs a run.
- **Accepted architecture:** recorded ADR/owner decision, even if no implementation exists.
- **Proposed:** KNOWER recommendation awaiting the appropriate decision maker.
- **Unverified:** not inspected or not executed in this session.
- **Known limitation:** explicit product, protocol or verification boundary.
- **Open question:** unresolved requirement or inconsistency; never silently treated as decided.

For each meaningful milestone, record the observation date, revision/working-tree boundary,
evidence, checks actually run, gaps and questions. Re-read live files before advising on a new task.

## Roles and file ownership

Dion's KNOWER assignment overrides the default implementer role for this thread. KNOWER does not
pick a ready implementation task, alter production code, change task statuses, approve architecture,
commit or push. Claude plans/reviews; Hardworker implements. KNOWER owns this directory, as recorded in HANDOFF §2. Contributors should avoid editing the same knowledge file simultaneously.

Shared active files including HANDOFF, ROADMAP, ARCHITECTURE and task specs were left untouched.
Consultation results requiring decisions must be incorporated into canonical documents by their
owner before implementation relies on them.

## Session coordination

| Agent | Session ID | Evidence |
|---|---|---|
| KNOWER | `01a19e2a-ebb0-7c21-8860-08f501a12e7e` | Both `CODEX_THREAD_ID` and `CODEX_SESSION_ID` in this process |
| Codex Hardworker | `01a19e0d-2c19-7810-8e5c-410055baffb6` | Owner assignment and HANDOFF §2; no takeover performed |
| Claude | `f196f16e-c456-4222-b61a-334553c1a267` | Owner assignment; no invocation performed |

At onboarding, the installed `codex-cli 0.166.0` advertised `queue` and `exec resume`. Its local help and the
[official CLI reference](https://learn.chatgpt.com/docs/cli/reference) establish these interfaces.
They do not prove that a recipient is running or has read a message.

Fallback CLI notification, after writing a complete request to QUESTIONS_FOR_KNOWER:

```sh
codex queue --thread 01a19e2a-ebb0-7c21-8860-08f501a12e7e --message 'KNOWER consultation: read request KQ-XXX in docs/knowledge/QUESTIONS_FOR_KNOWER.md. Answer in KNOWER_RESPONSES.md; no implementation changes.'
```

Queueing alone does not start a turn. `--resume` attempts activation through a local daemon or
selected remote server; its command can still succeed when resume fails. Record queue acceptance,
activation and an actual response separately. Do not launch overlapping writers for one thread.
`codex exec resume <id> <prompt>` is also advertised, but was not used to resume any session here.
For KNOWER, always retain the advisory-only role in the prompt.

Current runtime exposes `mcp__codex_tui__send_message_to_thread` for the existing Codex thread
IDs; KNOWER has used it to return a status refresh to Hardworker. Prefer that purpose-built
channel when available, with a short pointer to evidence. Tool acceptance and recipient
acknowledgement remain separate. Native messaging was unavailable during initial onboarding;
that historical limitation no longer describes the current runtime. Claude's CLI advertises
`--resume`, but that is execution rather than an inbox; it was not used as a notification channel.
Repository request/response files remain the dependable shared handoff. HANDOFF's Agent Mail
connection failure is historical documentation, not a fresh connectivity test by KNOWER.

Onboarding notification to Hardworker was accepted by `codex queue` (without `--resume`):
message `01a19e30-a56c-7433-ad28-acc2a455ea2d`. The sandboxed attempt failed to start the embedded
server; the authorized retry outside the sandbox returned this queue ID with exit 0. This proves
queue acceptance only at that time. Subsequently Hardworker explicitly acknowledged reading
the onboarding and Claude recorded receipt and the verified KNOWER ID in HANDOFF §2/§6.
Registration is complete. KNOWER did not edit that shared file concurrently.

KNOWER only processes requests during an active turn or an explicitly activated later turn;
there is no background monitor or promise of activity after the session stops.
