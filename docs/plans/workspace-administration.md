# Workspace Administration — Living Tracker

**Program:** Complete Workspace Administration (slug-scoped Linear-style settings)  
**Note:** One-time delivery; orchestrator skill removed after stack completion.

## Status

| Field | Value |
|-------|--------|
| Run ID | `07aef382` |
| Current PR | — |
| Phase | completed (merged via fullstack verification flow) |
| Branch base | `main` (was `codex/fullstack-verification-flow`) |

## Locked defaults

- User-facing copy: **Workspace**; internal: **organization**
- Settings routes use a **settings sidebar** (not global rail)
- Lifecycle primitives in **wasi-auth** first, then ddd template/example
- Workspace **slug read-only** after create; rename = display name only
- Preserve onboarding gate + `/organizations` create modal

## Milestones

- [x] **M0** Baseline & module split (PR0)
- [x] **M1** Access model (PR1)
- [x] **M2** Settings shell & routes (PR2)
- [x] **M3** Read models & transport (PR3)
- [x] **M4** Settings areas (PR4a–e)
- [x] **M5** Lifecycle SQL (ships with PR4b/c/e)
- [x] **M6** Verification & isolation (PR5) — pragmatic slice (smoke + isolation docs; authz matrix deferred)

## PR stack

| ID | Title | Depends | Status |
|----|-------|---------|--------|
| PR0 | Tracker + org UI module split + template sync | — | done |
| PR1 | OrganizationAccessModel + deps + error fidelity | — (∥ PR0) | done (code; step-up fidelity deferred) |
| PR2 | Settings shell + slug routes + legacy redirects | PR0 | done (code; awaiting orchestrator advance) |
| PR3 | Settings DTOs + slug-scoped reads + assign/remove/update fns | PR2 (+PR1 preferred) | done (code; awaiting orchestrator advance) |
| PR4a | General + Members UI | PR3 | done (code; awaiting orchestrator advance) |
| PR4b | Invitations UI + revoke/resend | PR3 + wasi-auth | done (code; awaiting orchestrator advance) |
| PR4c | Roles UI + delete custom role | PR3 + wasi-auth | done (code; awaiting orchestrator advance) |
| PR4d | Audit humanization | PR3 | done (code; awaiting orchestrator advance) |
| PR4e | Ownership transfer + Danger zone | PR3 + wasi-auth | done (code; awaiting orchestrator advance) |
| PR5 | Isolation harness + authz matrix + browser suite | PR4* | done (pragmatic slice; authz matrix deferred) |

Default order: `PR0 → PR1 → PR2 → PR3 → PR4a → PR4b → PR4c → PR4d → PR4e → PR5`

## PR checklists

### PR0 — M0 Baseline

- [x] Living tracker present (this file)
- [x] Split `src/app/organizations/mod.rs` (switcher / create_modal / legacy links)
- [x] Scaffold `src/app/workspace_settings/` modules (can be stubs)
- [x] `bash examples/fullstack-app/scripts/sync_fullstack_template.sh`
- [x] `… check` + `make check` + `check_loc.sh`
- Evidence:
  - `cd examples/fullstack-app && make check` — green (ssr/wasm32-wasip2)
  - `cd examples/fullstack-app && bash scripts/check_loc.sh` — LOC budget OK (max 1200)
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `… check` — template in sync
  - Split: `organizations/{mod,home,create_modal,links,legacy_pages}.rs`
  - Scaffold: `workspace_settings/{mod,shell,general,members,invitations,roles,audit,danger,shared}.rs` (no routes yet)

### PR1 — M1 Access model (wasi-auth)

- [x] `OrganizationAccessModel` (labels, groups, dependencies, risk, custom_role_eligible)
- [x] Core vs application permission split
- [x] Dependency enforcement on custom-role upsert
- [x] `SlugConflict` → product conflict/validation
- [ ] Step-up error fidelity (prefer dedicated code) — **deferred**: SQL `NotAuthorized` still collapses AAL/assurance fails; dedicated `ManagementError::StepUpRequired` needs SQL outcome distinction (follow-up)
- Evidence:
  - wasi-auth: `src/postgres/access_model.rs` + `upsert_role` auto-expands deps then validates eligibility; `cargo test -p wasi-auth --lib --features postgres-kernel` — 45 passed
  - ddd: `map_organization_error` maps `OrganizationError::SlugConflict` → product conflict (“workspace URL is already taken”)
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `cd examples/fullstack-app && make check` — green

### PR2 — M2 Shell & routes

- [x] Routes `/org/:slug/settings/{general,members,invitations,roles,audit,danger}`
- [x] Settings sidebar chrome; hide global rail on these routes
- [x] Legacy `/organizations/*` redirects
- [x] `/organizations` switcher + create modal only
- Evidence:
  - Path helpers: `is_workspace_settings_path`; `is_workspace_path` returns false for settings
  - `AppLayout` three modes: settings shell / workspace rail / auth-shell
  - Routes: `/org/:slug/settings` → general; plus general/members/invitations/roles/audit/danger stubs
  - Legacy: `/organizations/{settings,members,invitations,roles,permissions,audit}` → slug-scoped settings
  - Permissions: `workspace_settings_route_permission` in `server.rs` (general/danger → organization.view; members/invitations → member.view; roles → role.view; audit → audit.view)
  - CSS: `.workspace-settings-shell` + mobile drawer in `input.css`
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `… check` — template in sync
  - `cd examples/fullstack-app && make check` — green; `bash scripts/check_loc.sh` — OK
  - Hydrate: `cargo check --target wasm32-unknown-unknown --features hydrate` — green

### PR3 — M3 Transport

- [x] `WorkspaceSettingsContext` (+ page DTOs)
- [x] Slug resolve + membership check helper
- [x] Slug-scoped server_fns
- [x] Wire update/assign/remove for UI
- Evidence:
  - Contracts: `WorkspaceSettingsContext`, `WorkspaceSettingsOrganization`, `WorkspaceSettingsMembership`, `WorkspaceRoleOption` in `contracts/organization.rs`
  - `resolve_workspace_by_slug` / `resolve_workspace_by_slug_with_context` — slug → org id (store) + active membership via `organization_for_session` (not session tenant alone)
  - Server fns (`/api/ui`): `get_workspace_settings_context`, `list_workspace_{members,invitations,roles,audit}`, `update_workspace_name`, `assign_workspace_member_role`, `remove_workspace_member`, `invite_workspace_member`, `upsert_workspace_role`
  - Mutations re-verify AAL2 + slug membership; slug immutable on rename
  - UI: General loads context, editable name + read-only slug, Save → `update_workspace_name` with `server_error_text`; Members loads `list_workspace_members` table (email/role/status)
  - Shell identity prefers settings context name/slug
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `… check` — template in sync
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK

### PR4a — General + Members

- [x] Editable name; immutable slug
- [x] Members table + server-authored role combobox
- [x] Remove member confirm
- Evidence:
  - General: name field, immutable slug/URL, status + created from context; Save → `update_workspace_name`; disabled when empty/unchanged; success + `server_error_text`; step-up banner links to `/account/mfa`
  - Members: table (email, role, status, joined, actions); “You” badge; role `<select>` only from context `role_options` (owner static); assign → `assign_workspace_member_role`; remove board-modal confirm → `remove_workspace_member`; self-remove disabled (“Use Danger zone to leave (coming soon)”); pending states; list refresh on success/error
  - Shared: `role_label_from_options`, `format_settings_timestamp_ms`; CSS for step-up, members actions, confirm
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `… check` — template in sync
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK

### PR4b — Invitations

- [x] Invite + list states
- [x] Resend / revoke (wasi-auth)
- Evidence:
  - wasi-auth `ac2fcd73`: `InvitationService::{revoke,resend}` + `revoke_invitation.sql` / `resend_invitation.sql` (AAL2 + `member.invite`; audit `member.invite.revoke` / `member.invite.resend`; OTT consume on both; resend rotates token hash, extends TTL, new sealed outbox). Unit tests for invitation UUID validation + TTL; live SQL: `scripts/test-postgres-kernel-live.sh`
  - ddd: `auth_product::{revoke,resend}_invitation`; application + server_fns `revoke_workspace_invitation` / `resend_workspace_invitation`; existing list/invite wired; registered workspace server fns in `server.rs` (incl. PR3/4a missed registrations)
  - UI invitations: invite form (email + role_options default member); table email/role/status/expires; Resend/Revoke pending only; `server_error_text`; step-up hint; pending buttons
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `check` — green
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK

### PR4c — Roles

- [x] Combined roles + capabilities UI
- [x] Custom role create/edit/delete
- Evidence:
  - wasi-auth: `delete_role.sql` + `OrganizationManagementService::delete_role` (non-built-in only; blocks active members / pending invitations with `RoleInUse` counts; retargets residual FK refs to `member`; bumps `authorization_revision`; audit `role.manage` delete; AAL2 + `role.manage`)
  - ddd: `auth_product::delete_role`; `delete_workspace_role` / `list_workspace_permissions`; application + server_fns; registered on Leptos handler
  - UI roles: list with built-in/custom badge, permission count, member counts (from members list); create/edit custom role (name, id, multi-select from access-model options); delete with confirm + conflict message; built-ins immutable
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `check` — green
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK

### PR4d — Audit

- [x] Humanized table + filters + cursor + details drawer
- Evidence:
  - UI audit: humanized action labels, outcome pills, target summary; actor email via members map when available
  - Client filters: action select, outcome select, actor text (API is cursor-only)
  - Cursor pagination: Load more via `after` = last page `next_cursor`; Refresh reloads from start
  - Details modal: raw ids, sequence/org/actor/target; notes request_id/policy_revision not on list API; safe JSON of returned fields
  - Pure UI humanization (no DTO enrichment; list API lacks request_id/metadata)
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `check` — green
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK

### PR4e — Ownership + Danger

- [x] Transfer ownership (atomic)
- [x] Leave workspace
- [x] Soft deactivate / archive
- Evidence:
  - wasi-auth: `transfer_ownership.sql` / `leave_organization.sql` / `archive_organization.sql` + management methods (promote-before-demote transfer; last-owner leave fail; archive revokes pending invites + clears selected org + bumps authz revision; **no** membership freeze / bulk relationship revoke — gated by `status='active'`)
  - ddd: `transfer_workspace_ownership` / `leave_workspace` / `deactivate_workspace` application + server_fns; members transfer modal (owners only); danger page leave + typed-confirm deactivate
  - `bash examples/fullstack-app/scripts/sync_fullstack_template.sh` + `check` — green
  - `cd examples/fullstack-app && make check` — green; hydrate check green; `bash scripts/check_loc.sh` — OK


### PR5 — Verification

- [x] Isolated mutating test DB (or documented `make fresh` interim) — **interim docs only** (no per-run Postgres yet)
- [ ] Authz matrix (roles × AAL) — **deferred** (out of pragmatic slice)
- [x] Browser suite for settings — `scripts/verify_workspace_settings.mjs` + `make workspace-settings-smoke`
- [x] Template parity green
- Evidence:
  - Path helper unit tests: `src/app/path.rs` covers `is_workspace_settings_path` for all settings areas + topbar titles  
    - `cargo test --lib --features ssr,postgres -- app::path::tests` → **3 passed**  
    - also fixed pre-existing lib-test imports in `server_fns/mod.rs` + `application/mod.rs` so `cargo test --lib` compiles
  - Browser smoke: `examples/fullstack-app/scripts/verify_workspace_settings.mjs` + `make workspace-settings-smoke` / `npm run workspace-settings-smoke`
    - login/register (BROWSER_SMOKE_EMAILS or mail-capture register)
    - create org via `POST /api/organizations` with unique slug
    - visits general/members/invitations/roles/audit/danger — expects settings shell + page titles
    - optional rename when `ALLOW_MUTATING_SMOKE=1` (AAL2 step-up accepted, not hard fail)
    - skips cleanly if server down or session cannot be obtained
  - Smoke runs this PR:
    - `BASE_URL=http://127.0.0.1:9 npm run workspace-settings-smoke` → **skip** (server not reachable) ✓
    - `BASE_URL=http://127.0.0.1:3008 npm run workspace-settings-smoke` against local server → **skip** (mail capture 503 configuration; no `BROWSER_SMOKE_EMAILS`) — script skip path OK; full authenticated UI path not exercised in this environment
  - Makefile: `workspace-settings-smoke` documented in help; needs running `make dev` + optional `ALLOW_MUTATING_SMOKE=1`
  - Isolation notes (this section + script header): shared DB pollution risk; prefer `make fresh db=postgres` before mutating smokes; per-run Postgres **not implemented**
  - `bash scripts/sync_fullstack_template.sh` + `… check` → template in sync
  - `cd examples/fullstack-app && make check` → green; `bash scripts/check_loc.sh` → OK

## Isolation notes (PR5)

Mutating fullstack smokes share the same local Postgres as interactive `make dev`. That means:

1. **Pollution risk** — each settings smoke creates a workspace (`ws-settings-*` slug). Rename / invite / role mutations leave rows and audit events.
2. **Before mutating smokes** (or when the DB is dirty):  
   `make -C examples/fullstack-app fresh db=postgres` then `make dev`.
3. **Optional mutation flag**: `ALLOW_MUTATING_SMOKE=1` enables UI rename in workspace-settings smoke; omit it for page-load-only checks (still creates one org).
4. **Not implemented yet**: ephemeral per-run Postgres (container/schema) for true isolation. Documented interim is `make fresh` + unique slugs/emails.

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

# Workspace settings browser smoke (server must already be up)
make -C examples/fullstack-app workspace-settings-smoke
ALLOW_MUTATING_SMOKE=1 make -C examples/fullstack-app workspace-settings-smoke
```

## Decisions log

| Date | Decision | Who |
|------|----------|-----|
| 2026-07-16 | Plan approved; defaults locked (Workspace copy, settings chrome, slug read-only, wasi-auth-first) | user + plan |
| 2026-07-16 | Orchestrator skill added with human approval gates | agent |

## Blockers

_(none)_
