# Tailwind migration tracker

**Status:** in progress  
**Branch:** `codex/fullstack-verification-flow`  
**Policy:** dual-sync `examples/fullstack-app` ↔ `crates/ddd-cli/templates/fullstack` after every completed phase. Local commits only unless human asks to push.

## Done criteria

- [ ] `input.css` is small: `@import "tailwindcss"`, `@source`, `@theme` / tokens, optional `@custom-variant` — **0 residual semantic CSS**
- [ ] Markup uses Tailwind utilities (or `src/ui/*` that expand to utilities)
- [ ] No `.auth-*` / `.board-*` / `.workspace-*` / `.primary-button` / … definitions left
- [ ] Light/dark via `prefers-color-scheme` unchanged in intent
- [ ] `make check` green; dual-sync clean; relevant Playwright smokes green

## Phases

| Phase | Description | Status |
|-------|-------------|--------|
| 0 | Tailwind v4 entry; coexistence with legacy CSS; this tracker; structural `data-testid`s | **done** |
| 1 | Design tokens → `@theme inline` bridged to CSS vars | **done** |
| 2 | `src/ui/*` primitives emit utilities (`classes.rs`) | **done** |
| 3.1 | Auth surfaces (pages + forms → constants) | **done** (legacy CSS still present until unused) |
| 3.2 | Public / error shells | **done** |
| 3.3–3.5 | Account domain (MFA/passkeys/vault/profile/sessions/providers) → utilities | **done** (account CSS deleted; shared mono/kv/modal CSS remains for other domains) |
| 3.6 | Organizations shared buttons/fields | **done** (orgs list/create modal pure Tailwind; CSS deleted) |
| 3.7–3.8 | Dashboard board + resources shared primitives | **done** (board/resources CSS deleted; scroll-lock class + `board-pulse` keyframes remain) |
| 3.9 | Workspace settings shared primitives | **done** (domain pages pure Tailwind; shell chrome residual until Phase 4/5) |
| 3.10–3.11 | Admin + onboarding | **done** (admin forms/KV/inline-field + onboarding/slug-input utilities; domain CSS deleted) |
| 4 | Workspace shell (sidebar modes, drawer, flyouts) | **done** |
| 5 | Closeout: delete residual CSS, update smokes, dual-sync, docs | pending |

## Progress notes

- 2026-07-16: Phase 0–2 landed. Tailwind v4 `@import`, `@theme inline` color bridge, base layer, `src/ui/classes.rs`, primitives + shells rewrite.
- 2026-07-16: Auth pages/forms + bulk shared-class swap across account/dashboard/settings/admin. `make check` green. Dual-sync after foundation.
- 2026-07-16: Account domain pure Tailwind — MFA/passkeys/vault/profile/sessions/providers markup → `classes.rs` utilities; deleted matching account CSS. Left `.vault-modal-confirm` stub + shared `.mono-value`/`.kv`/board-modal for other domains.
- 2026-07-16: Dashboard board + resources pure Tailwind — `dashboard/board/*` + `resources.rs` → `BOARD_*` / shared constants; deleted board page/tile/rq CSS. Kept `.board-modal*` / `.board-muted` + `@keyframes board-pulse` for org-create residual + live-dot animation (settings later moved to `VAULT_MODAL_*`).
- 2026-07-16: Workspace settings domain pure Tailwind — `workspace_settings/{general,members,invitations,roles,audit,danger,shared}` → `WS_*` / vault modal constants; mobile card tables + danger full-width via utilities; deleted domain settings CSS + filter-combobox residual. Kept settings shell chrome (sidebar/nav/topbar/content) + org `board-modal*`.
- 2026-07-16: Organizations + admin + onboarding pure Tailwind — `organizations/**`, `admin/**`, onboarding panel/page, slug-input → `ORG_*` / `ONBOARDING_*` / `SLUG_INPUT_*` / shared `KV_LIST`/`INLINE_FIELD`/`TEXTAREA`; create modal off residual `board-modal*`; deleted orgs/admin/onboarding/dash/board-modal (except `html.board-modal-open` scroll lock + `@keyframes board-pulse`). Workspace shell residual remains for Phase 4.
- 2026-07-16: Workspace shell pure Tailwind — `workspace/mod.rs` + org switcher + user menu → `WS_*` / `ORG_SWITCHER_*` / `USER_MENU_*` utilities; `@custom-variant shell-mini|shell-hidden|shell-animated` (desktop ≥961px + FOUC `html[data-sidebar-pref]`); peer/checkbox mobile drawer; inline SVG nav icons (no CSS masks); mini account flyout fixed `left:76px` via utilities; deleted workspace shell / org-switcher / user-menu CSS. Tiny residual: `body:has(#workspace-nav-toggle:checked){overflow:hidden}`. Settings shell chrome residual remains for Phase 5.
- Next: residual closeout (settings shell, auth/page/home dead CSS, smokes → data-testid).

## Residual CSS allowlist

During migration, everything under the `LEGACY SEMANTIC CSS` banner in `input.css` is allowlisted. Shrink by domain; do not add new semantic rules.

Target residual after Phase 5: **none** (tokens/base only).

## Structural test ids

| `data-testid` | Element |
|---------------|---------|
| `workspace-shell` | Main workspace chrome root |
| `workspace-settings-shell` | Slug-scoped settings shell |
| `auth-page` | Auth page wrapper |
| `auth-card` | Auth card |

Prefer these (and roles) in Playwright over semantic class names.

## Dual-sync

```bash
bash examples/fullstack-app/scripts/sync_fullstack_template.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh check
```

## Reference

- Plan: session plan (pure Tailwind refactor)
- Pattern: `examples/counter-app/input.css` (already pure v4)
- Pre-work: `REFACTOR_GOAL.md` modularization complete
