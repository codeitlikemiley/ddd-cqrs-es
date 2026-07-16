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
| 3.6 | Organizations shared buttons/fields | **partial** |
| 3.7–3.8 | Dashboard board + resources shared primitives | **partial** |
| 3.9 | Workspace settings shared primitives | **partial** |
| 3.10–3.11 | Admin + onboarding | **partial** |
| 4 | Workspace shell (sidebar modes, drawer, flyouts) | pending |
| 5 | Closeout: delete residual CSS, update smokes, dual-sync, docs | pending |

## Progress notes

- 2026-07-16: Phase 0–2 landed. Tailwind v4 `@import`, `@theme inline` color bridge, base layer, `src/ui/classes.rs`, primitives + shells rewrite.
- 2026-07-16: Auth pages/forms + bulk shared-class swap across account/dashboard/settings/admin. `make check` green. Dual-sync after foundation.
- 2026-07-16: Account domain pure Tailwind — MFA/passkeys/vault/profile/sessions/providers markup → `classes.rs` utilities; deleted matching account CSS. Left `.vault-modal-confirm` stub + shared `.mono-value`/`.kv`/board-modal for other domains.
- Next: finish domain-specific class strings (board-*, workspace-*, org-*), then delete matching CSS blocks from `input.css`.

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
