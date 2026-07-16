# Tailwind migration tracker

**Status:** complete  
**Branch:** `execute-plan/710bca15-pr-6-residual-css-purge-and-closeout`  
**Policy:** dual-sync `examples/fullstack-app` ↔ `crates/ddd-cli/templates/fullstack` after every completed phase. Local commits only unless human asks to push.

## Done criteria

- [x] `input.css` is small: `@import "tailwindcss"`, `@source`, `@theme` / tokens, optional `@custom-variant` — **0 residual semantic CSS**
- [x] Markup uses Tailwind utilities (or `src/ui/*` that expand to utilities)
- [x] No `.auth-*` / `.board-*` / `.workspace-*` / `.primary-button` / … definitions left
- [x] Light/dark via `prefers-color-scheme` unchanged in intent
- [x] `make check` green; dual-sync clean; relevant Playwright smokes green

## Phases

| Phase | Description | Status |
|-------|-------------|--------|
| 0 | Tailwind v4 entry; coexistence with legacy CSS; this tracker; structural `data-testid`s | **done** |
| 1 | Design tokens → `@theme inline` bridged to CSS vars | **done** |
| 2 | `src/ui/*` primitives emit utilities (`classes.rs`) | **done** |
| 3.1 | Auth surfaces (pages + forms → constants) | **done** |
| 3.2 | Public / error shells | **done** |
| 3.3–3.5 | Account domain (MFA/passkeys/vault/profile/sessions/providers) → utilities | **done** |
| 3.6 | Organizations shared buttons/fields | **done** |
| 3.7–3.8 | Dashboard board + resources shared primitives | **done** |
| 3.9 | Workspace settings shared primitives | **done** |
| 3.10–3.11 | Admin + onboarding | **done** |
| 4 | Workspace shell (sidebar modes, drawer, flyouts) | **done** |
| 5 | Closeout: delete residual CSS, update smokes, dual-sync, docs | **done** |

## Progress notes

- 2026-07-16: Phase 0–2 landed. Tailwind v4 `@import`, `@theme inline` color bridge, base layer, `src/ui/classes.rs`, primitives + shells rewrite.
- 2026-07-16: Auth pages/forms + bulk shared-class swap across account/dashboard/settings/admin. `make check` green. Dual-sync after foundation.
- 2026-07-16: Account domain pure Tailwind — MFA/passkeys/vault/profile/sessions/providers markup → `classes.rs` utilities; deleted matching account CSS.
- 2026-07-16: Dashboard board + resources pure Tailwind — `dashboard/board/*` + `resources.rs` → `BOARD_*` / shared constants; deleted board page/tile/rq CSS.
- 2026-07-16: Workspace settings domain pure Tailwind — domain pages → `WS_*` / vault modal constants; deleted domain settings CSS.
- 2026-07-16: Organizations + admin + onboarding pure Tailwind — domain CSS deleted.
- 2026-07-16: Workspace shell pure Tailwind — `workspace/mod.rs` + org switcher + user menu → utilities; `@custom-variant shell-mini|shell-hidden|shell-animated`.
- 2026-07-16: **Phase 5 closeout** — settings shell → `WSS_*` utilities; home/onboarding residual purged; Playwright smokes prefer `data-testid`; `input.css` pure Tailwind (+ minimal non-semantic residuals: body:has drawer scroll lock, `html.board-modal-open` scroll lock, `@keyframes board-pulse`).

## Residual CSS allowlist

**Closed.** No semantic class rules remain in `input.css`.

Allowed non-semantic residuals only:

| Residual | Why |
|----------|-----|
| `body:has(#workspace-nav-toggle:checked)` / settings toggle | Mobile drawer scroll lock via checkbox peer |
| `html.board-modal-open` | JS toggles class on `<html>` for modal scroll lock |
| `@keyframes board-pulse` | Named keyframes for `animate-[board-pulse_…]` live-dot utility |

## Structural test ids

| `data-testid` | Element |
|---------------|---------|
| `workspace-shell` | Main workspace chrome root |
| `workspace-settings-shell` | Slug-scoped settings shell |
| `auth-page` | Auth page wrapper |
| `auth-card` | Auth card |
| `account-page` | Account settings column |
| `error-page` | Error interrupt shell |

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
