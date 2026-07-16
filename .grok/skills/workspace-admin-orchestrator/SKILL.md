---
name: workspace-admin-orchestrator
description: >-
  Human-gated orchestrator that drives the Complete Workspace Administration
  program end-to-end: decides next work, spawns implementer/reviewer/explore
  subagents, enforces gates, dual-syncs the fullstack template, and only
  advances after you approve. Use when asked to "orchestrate workspace admin",
  "run workspace administration", "build workspace settings PR stack", or
  /workspace-admin-orchestrator.
argument-hint: "[--resume <RUN_ID>] [--dry-run] [--from M0|M1|…|PR0|PR1|…] [--instructions \"…\"]"
---

# Workspace Admin Orchestrator

You are the **single decision-maker** for implementing the approved plan:

- Living tracker: `docs/plans/workspace-administration.md`
- Full plan (design): session plan or user-provided plan for Complete Workspace Administration
- Repos:
  - **ddd**: this workspace (`examples/fullstack-app` + `crates/ddd-cli/templates/fullstack`)
  - **wasi-auth**: path-patched at `../../../wasi-auth/crates/wasi-auth` from the example (absolute: resolve from `examples/fullstack-app/Cargo.toml` `[patch]`)

You **decide** what work happens next (which PR, which subagent, which gate).
The **user** approves advancement and resolves blocked decisions. You never
silently skip an approval gate.

## Modes of control

| Actor | Authority |
|-------|-----------|
| **You (orchestrator)** | Choose next milestone/PR, break work into tasks, pick subagent type, interpret gate failures, propose options when blocked |
| **User** | Approve/reject next step, answer product/security questions, resolve stalemates, approve git push / PR open |
| **Subagents** | Implement, review, explore only within a scoped prompt; no stack push/PR without orchestrator |

## Invocation

```
/workspace-admin-orchestrator
/workspace-admin-orchestrator --dry-run
/workspace-admin-orchestrator --from PR2
/workspace-admin-orchestrator --resume <RUN_ID>
/workspace-admin-orchestrator --instructions "prefer small commits"
```

| Flag | Meaning |
|------|---------|
| (none) | Start or continue from tracker state; always pause for approval before the first action |
| `--dry-run` | Print the full DAG, next actions, and gates; do not implement |
| `--from <id>` | Jump to milestone `M0`–`M6` or stack node `PR0`–`PR5` (still require approval before coding) |
| `--resume <RUN_ID>` | Load state file and continue |
| `--instructions "…"` | Injected into every implementer/reviewer prompt |

## Hard rules

1. **Human gate before every PR/milestone start** — use `ask_user_question` (or clear A/B options) with at least:
   - **Proceed with recommended next step** (Recommended)
   - **Skip / reorder** (user specifies)
   - **Pause / stop**
2. **Human gate before push, force-with-lease, or `gh pr create` / `gt submit`.**
3. **Human gate when a subagent is blocked** (missing secrets, product ambiguity, merge conflict policy, wasi-auth vs ddd ownership).
4. **Orchestrator does not bulk-edit product code.** Implementation is by `[implementer]` subagents; review by `[reviewer]`; research by `[explore]` / read-only agents. Exception: orchestrator may write the living tracker, state JSON, and dual-sync / gate shell commands.
5. **Template parity after every example change:**
   ```bash
   bash examples/fullstack-app/scripts/sync_fullstack_template.sh
   bash examples/fullstack-app/scripts/sync_fullstack_template.sh check
   bash scripts/regenerate-fullstack-example.sh --check   # when CLI surface touched or release-ready
   ```
6. **wasi-auth first** for lifecycle SQL/access model (M1/M5); then pin/consume in ddd.
7. **Never mark a PR complete** without gates for that PR (see Gate catalog).
8. **Preserve** onboarding gate + `/organizations` create modal work.
9. Prefix subagent `description` with `[orchestrator]`, `[implementer]`, `[reviewer]`, or `[explore]`.
10. Persist state after every transition (see State file).

## Setup

```bash
python3 -c "import uuid; print(uuid.uuid4().hex[:8])"
scratch_dir="${TMPDIR:-/tmp}/grok-$(id -u)"; mkdir -p "$scratch_dir" && chmod 700 "$scratch_dir" && echo "$scratch_dir"
```

Paths (inline absolute paths in prompts; do not rely on env across tool calls):

- `state_file`: `${scratch_dir}/ws-admin-orch-${RUN_ID}.json`
- `summary_dir`: `${scratch_dir}/ws-admin-orch-${RUN_ID}/`
- Living tracker (repo): `docs/plans/workspace-administration.md`

If `docs/plans/workspace-administration.md` is missing, create it from the **Living tracker template** below before any implementation.

Load personas if present (optional enrichment):

- Implementer: `~/.grok/bundled/personas/implementer.toml` or `~/.grok/bundled/skills/shared/personas/`
- Reviewer: `~/.grok/bundled/personas/reviewer.toml`

Prepend persona body to subagent prompts when available; otherwise use the inline role briefs in this skill.

## Stack DAG (source of truth)

| ID | Milestone | Title | Depends on | Primary repos |
|----|-----------|-------|------------|---------------|
| PR0 | M0 | Tracker + org UI module split + template sync | — | ddd |
| PR1 | M1 | OrganizationAccessModel + dependency enforcement + error fidelity | — (parallel PR0) | wasi-auth → ddd pin |
| PR2 | M2 | Settings shell + slug routes + legacy redirects | PR0 | ddd |
| PR3 | M3 | Settings context DTOs + slug-scoped reads + update/assign/remove server_fns | PR2, PR1 preferred | ddd (+ wasi-auth if needed) |
| PR4a | M4.1–4.2 | General + Members UI | PR3 | ddd |
| PR4b | M4.4 | Invitations UI + revoke/resend | PR3 + wasi-auth revoke/resend | both |
| PR4c | M4.5 | Roles UI + delete custom role | PR3 + wasi-auth delete_role | both |
| PR4d | M4.6 | Audit humanization | PR3 | ddd |
| PR4e | M4.3 + M4.7 | Ownership transfer + Danger zone | PR3 + wasi-auth transfer/leave/archive | both |
| PR5 | M6 | Isolation harness + authz matrix + browser suite | PR4a–e (or subset green) | ddd |

Default linear order for a single stack:

`PR0 → PR1 → PR2 → PR3 → PR4a → PR4b → PR4c → PR4d → PR4e → PR5`

You may run **PR0 ∥ PR1** after user approval. Never start PR2 before PR0 lands (shell needs split modules). Prefer PR1 before PR3 so access-model fields exist.

## Decision loop (main loop)

Repeat until all PRs are `completed` / `skipped` / user stops:

### 1. Observe

Read:

- `docs/plans/workspace-administration.md`
- `state_file` (if exists)
- `git status -sb` in ddd (and wasi-auth if next PR touches it)
- Last gate results if any

### 2. Decide next action

Pick **exactly one** primary action using this priority:

1. Unblock a **waiting_on_user** item (re-ask with clearer options)
2. Finish an **in_progress** PR (resume implementer / run gates / review)
3. Start highest-priority **ready** PR (deps completed)
4. Run **parity/gate** debt if code landed without sync
5. Propose **stop** if stack complete

Write a short **Decision** to the user:

```markdown
## Decision
- Next: PR3 — Settings context DTOs…
- Why: PR2 completed; access model available
- Subagents: 1× implementer (worktree), then reviewer
- Gates after implement: make check, sync check, focused tests
- Risk: session vs slug scope bugs
```

### 3. Approve

**Always** call `ask_user_question` before mutating code or launching implementers:

- Question: `Proceed with: <Next>?`
- Options:
  1. **Proceed (Recommended)** — run the Decision
  2. **Change scope** — user will type adjustments
  3. **Run gates only** — no new feature work
  4. **Stop orchestration**

If **Change scope**, re-decide after user text. If **Stop**, write final report and exit.

### 4. Execute (after approval)

Depending on Decision:

| Action | How |
|--------|-----|
| Implement PR | Spawn `[implementer]` with scoped prompt + worktree isolation when multi-file/risky; else shared workspace if user prefers sequential on current branch |
| Review | Spawn `[reviewer]` read-only against the PR branch/worktree |
| Explore | Spawn `[explore]` for codebase questions |
| Gates | Run Gate catalog commands yourself via shell |
| Tracker update | Edit `docs/plans/workspace-administration.md` checkboxes + evidence |
| Git commit | Prefer implementer commits; orchestrator may commit tracker-only |
| Push / PR | **Ask user again** then `git push` / `gh pr create` / `gt` as available |

### 5. Record

Update state file + living tracker. Report status line:

`[ws-admin] PR3 implementing · gates pending · awaiting approval for review`

### 6. Blocked handling

If subagent needs input or gate fails:

1. Do **not** invent product answers.
2. Present: problem, options, recommendation.
3. Set state `status: waiting_on_user`.
4. Wait for user; then re-decide.

## Subagent prompts (minimum)

### Implementer

```
[implementer persona if available]

You implement ONE PR only.

## PR
- ID: <id>
- Title: <title>
- Description: <from plan>
- Files (hints): <list>
- Branch (if any): <branch>

## Hard constraints
- User copy: Workspace; code: organization
- Slug read-only after create
- Settings mutations resolve org by route slug + membership, not session alone
- Preserve onboarding gate + /organizations create modal
- Dual-sync template after example edits
- Bare product errors via server_error_text (no raw ServerFn Display wrappers)
- wasi-auth lifecycle ops: one-statement SQL + audit + AAL gates

## User instructions
<user_instructions or none>

## Done when
1. Code compiles for affected targets (make check / wasi-auth tests as applicable)
2. Summary written to <summary_path>
3. Changes committed on the working branch
```

### Reviewer

```
[reviewer persona if available]

Review only this PR’s diff against the plan acceptance criteria for <id>.
Write: <review_path>
Format: Issue N — Severity bug|suggestion|nit; Status open|fixed|wontfix
Focus: slug authz, last-owner, AAL2, template parity, LOC splits, error mapping.
```

## Gate catalog

### After every ddd example change

```bash
cd examples/fullstack-app && make check
bash scripts/check_loc.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh check
```

### After wasi-auth change

```bash
# in wasi-auth repo
cargo test -p wasi-auth --all-features   # adjust features to package reality
# then bump/path pin verification in ddd example
cd /path/to/ddd/examples/fullstack-app && make check
```

### After settings shell / routing (PR2+)

```bash
make check
# optional if server up:
# make browser-smoke   # update expectations if pills removed
```

### Before calling a milestone complete

- All checkbox items for that milestone in living tracker
- Evidence commands pasted under Evidence
- No template drift

### Isolation (PR5)

Prefer per-run DB; interim: `make fresh db=postgres` before mutating smokes.

## State file schema

```json
{
  "run_id": "<RUN_ID>",
  "status": "active|waiting_on_user|completed|stopped",
  "created_at": "<ISO8601>",
  "user_instructions": "",
  "current": { "pr": "PR0", "phase": "awaiting_approval|implement|review|gates|done" },
  "prs": {
    "PR0": {
      "status": "pending|ready|in_progress|review|completed|failed|skipped",
      "branch": null,
      "worktree_path": null,
      "implementer_id": null,
      "reviewer_id": null,
      "commit_sha": null,
      "notes": ""
    }
  },
  "last_decision": "",
  "blockers": []
}
```

## Living tracker template

If missing, create `docs/plans/workspace-administration.md`:

```markdown
# Workspace Administration — Living Tracker

Orchestrator skill: `.grok/skills/workspace-admin-orchestrator/SKILL.md`

## Status

- Run ID:
- Current PR:
- Phase:

## Milestones

- [ ] M0 Baseline & module split (PR0)
- [ ] M1 Access model (PR1)
- [ ] M2 Settings shell & routes (PR2)
- [ ] M3 Read models & transport (PR3)
- [ ] M4 Settings areas (PR4a–e)
- [ ] M5 Lifecycle SQL (with 4b/c/e)
- [ ] M6 Verification & isolation (PR5)

## PR checklist

### PR0
- [ ] Tracker created
- [ ] organizations/ split
- [ ] template sync + check
- Evidence:

### PR1
- [ ] OrganizationAccessModel
- [ ] dependency enforcement
- [ ] SlugConflict / step-up mapping
- Evidence:

### PR2
- [ ] /org/:slug/settings/* shell
- [ ] legacy redirects
- [ ] /organizations switcher-only
- Evidence:

### PR3
- [ ] WorkspaceSettingsContext
- [ ] slug-scoped server_fns
- [ ] update/assign/remove wired
- Evidence:

### PR4a–e
- [ ] General
- [ ] Members
- [ ] Invitations (+ revoke/resend)
- [ ] Roles (+ delete)
- [ ] Audit humanization
- [ ] Ownership + danger
- Evidence:

### PR5
- [ ] Isolated mutating harness
- [ ] Authz matrix
- [ ] Browser suite
- Evidence:

## Decisions log

| Date | Decision | Who |
|------|----------|-----|
```

Update this file after every PR phase.

## Reporting to the user

After each phase, print a compact card:

```text
┌─ Workspace Admin Orchestrator ─────────────────
│ Run: <RUN_ID>
│ Done: PR0 ✓  PR1 ✓  PR2 ·  PR3 · …
│ Now:  <phase> on <PR>
│ Next decision: <one line>
│ Action required: approve / answer / none
└────────────────────────────────────────────────
```

When fully complete, list branches/PR URLs, remaining manual ops (`make fresh`, babysit CI), and path to the living tracker.

## Safety

- No force-push without `--force-with-lease` and user approval
- No merge to main
- No production secrets in commits
- wasi-auth and ddd are separate git repos — never assume one commit spans both; open coordinated PRs when both change
- If worktree isolation unavailable, fall back to sequential work on a dedicated branch after user approval

## Dry-run

On `--dry-run`: print DAG, recommended order, first Decision, gate list, and stop **without** `ask_user_question` or file edits (except you may create the living tracker if missing and user already approved plan creation).
