# Workspace Administration — Living Tracker

**Orchestrator skill:** `.grok/skills/workspace-admin-orchestrator/SKILL.md`  
**Invoke:** `/workspace-admin-orchestrator`  
**Program:** Complete Workspace Administration (slug-scoped Linear-style settings)

## Status

| Field | Value |
|-------|--------|
| Run ID | _(set by orchestrator)_ |
| Current PR | — |
| Phase | not_started |
| Branch base | `codex/fullstack-verification-flow` (or main when stacking) |

## Locked defaults

- User-facing copy: **Workspace**; internal: **organization**
- Settings routes use a **settings sidebar** (not global rail)
- Lifecycle primitives in **wasi-auth** first, then ddd template/example
- Workspace **slug read-only** after create; rename = display name only
- Preserve onboarding gate + `/organizations` create modal

## Milestones

- [ ] **M0** Baseline & module split (PR0)
- [ ] **M1** Access model (PR1)
- [ ] **M2** Settings shell & routes (PR2)
- [ ] **M3** Read models & transport (PR3)
- [ ] **M4** Settings areas (PR4a–e)
- [ ] **M5** Lifecycle SQL (ships with PR4b/c/e)
- [ ] **M6** Verification & isolation (PR5)

## PR stack

| ID | Title | Depends | Status |
|----|-------|---------|--------|
| PR0 | Tracker + org UI module split + template sync | — | pending |
| PR1 | OrganizationAccessModel + deps + error fidelity | — (∥ PR0) | pending |
| PR2 | Settings shell + slug routes + legacy redirects | PR0 | pending |
| PR3 | Settings DTOs + slug-scoped reads + assign/remove/update fns | PR2 (+PR1 preferred) | pending |
| PR4a | General + Members UI | PR3 | pending |
| PR4b | Invitations UI + revoke/resend | PR3 + wasi-auth | pending |
| PR4c | Roles UI + delete custom role | PR3 + wasi-auth | pending |
| PR4d | Audit humanization | PR3 | pending |
| PR4e | Ownership transfer + Danger zone | PR3 + wasi-auth | pending |
| PR5 | Isolation harness + authz matrix + browser suite | PR4* | pending |

Default order: `PR0 → PR1 → PR2 → PR3 → PR4a → PR4b → PR4c → PR4d → PR4e → PR5`

## PR checklists

### PR0 — M0 Baseline

- [ ] Living tracker present (this file)
- [ ] Split `src/app/organizations/mod.rs` (switcher / create_modal / legacy links)
- [ ] Scaffold `src/app/workspace_settings/` modules (can be stubs)
- [ ] `bash examples/fullstack-app/scripts/sync_fullstack_template.sh`
- [ ] `… check` + `make check` + `check_loc.sh`
- Evidence:

### PR1 — M1 Access model (wasi-auth)

- [ ] `OrganizationAccessModel` (labels, groups, dependencies, risk, custom_role_eligible)
- [ ] Core vs application permission split
- [ ] Dependency enforcement on custom-role upsert
- [ ] `SlugConflict` → product conflict/validation
- [ ] Step-up error fidelity (prefer dedicated code)
- Evidence:

### PR2 — M2 Shell & routes

- [ ] Routes `/org/:slug/settings/{general,members,invitations,roles,audit,danger}`
- [ ] Settings sidebar chrome; hide global rail on these routes
- [ ] Legacy `/organizations/*` redirects
- [ ] `/organizations` switcher + create modal only
- Evidence:

### PR3 — M3 Transport

- [ ] `WorkspaceSettingsContext` (+ page DTOs)
- [ ] Slug resolve + membership check helper
- [ ] Slug-scoped server_fns
- [ ] Wire update/assign/remove for UI
- Evidence:

### PR4a — General + Members

- [ ] Editable name; immutable slug
- [ ] Members table + server-authored role combobox
- [ ] Remove member confirm
- Evidence:

### PR4b — Invitations

- [ ] Invite + list states
- [ ] Resend / revoke (wasi-auth)
- Evidence:

### PR4c — Roles

- [ ] Combined roles + capabilities UI
- [ ] Custom role create/edit/delete
- Evidence:

### PR4d — Audit

- [ ] Humanized table + filters + cursor + details drawer
- Evidence:

### PR4e — Ownership + Danger

- [ ] Transfer ownership (atomic)
- [ ] Leave workspace
- [ ] Soft deactivate / archive
- Evidence:

### PR5 — Verification

- [ ] Isolated mutating test DB (or documented `make fresh` interim)
- [ ] Authz matrix (roles × AAL)
- [ ] Browser suite for settings
- [ ] Template parity green
- Evidence:

## Gate commands (cheat sheet)

```bash
# Every ddd example change
cd examples/fullstack-app && make check
bash scripts/check_loc.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh check

# Parity / CI-style
bash scripts/regenerate-fullstack-example.sh --check

# Shared smoke (after fresh when mutating)
make -C examples/fullstack-app fresh db=postgres
make -C examples/fullstack-app dev transport=both
make -C examples/fullstack-app smoke
make -C examples/fullstack-app browser-smoke
```

## Decisions log

| Date | Decision | Who |
|------|----------|-----|
| 2026-07-16 | Plan approved; defaults locked (Workspace copy, settings chrome, slug read-only, wasi-auth-first) | user + plan |
| 2026-07-16 | Orchestrator skill added with human approval gates | agent |

## Blockers

_(none)_
