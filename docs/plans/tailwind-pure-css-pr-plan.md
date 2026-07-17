# Pure Tailwind CSS Migration — PR Plan

**Design source:** session plan + `examples/fullstack-app/TAILWIND_MIGRATION.md`  
**Base branch:** `codex/fullstack-verification-flow` (feature branch; not `main`)  
**Product:** `examples/fullstack-app` dual-synced to `crates/ddd-cli/templates/fullstack`  
**Policy:** UI-only; preserve behavior/a11y; `make check` + dual-sync after each PR.

## Context

Foundation (Tailwind v4 entry, `@theme` tokens, `src/ui/classes.rs`, shared primitives, auth pages/forms bulk swap) is committed on the base branch before this stack. Remaining work: domain-specific semantic classes → utilities, then delete legacy CSS from `input.css` (~7k LOC residual).

## Done criteria (stack)

- `input.css` ≈ tokens/base only (no semantic `.board-*` / `.workspace-*` / `.mfa-*` / …)
- Markup uses Tailwind utilities or `ui/classes` constants
- `make check` green; dual-sync clean
- Smokes use `data-testid` / roles where practical

## PR Plan

### PR 1: Account domain Tailwind utilities

- **Description:** Convert remaining account-domain semantic classes (MFA wizard, passkeys, vault modals, profile-specific, sessions chrome, providers) to Tailwind utilities / `ui/classes` constants. Delete matching CSS blocks from `input.css` when unused. Dual-sync template. `make check`.
- **Files/components affected:** `examples/fullstack-app/src/app/account/**`, `examples/fullstack-app/input.css`, `examples/fullstack-app/src/ui/classes.rs` (if needed), dual-sync via `scripts/sync_fullstack_template.sh`, `TAILWIND_MIGRATION.md`
- **Dependencies:** None

### PR 2: Dashboard board and resources Tailwind

- **Description:** Convert `board-*` and resources modal semantic classes to utilities. Delete board/resources CSS blocks from `input.css`. Dual-sync. `make check`.
- **Files/components affected:** `examples/fullstack-app/src/app/dashboard/**`, `examples/fullstack-app/input.css`, `examples/fullstack-app/src/ui/classes.rs`, dual-sync, `TAILWIND_MIGRATION.md`
- **Dependencies:** PR 1

### PR 3: Workspace settings Tailwind

- **Description:** Convert workspace settings shell, tables, members/roles/invitations/audit/danger domain classes to utilities. Preserve mobile card layout and danger full-width. Delete settings CSS. Dual-sync. `make check`.
- **Files/components affected:** `examples/fullstack-app/src/app/workspace_settings/**`, `examples/fullstack-app/input.css`, dual-sync, smoke scripts if selectors change, `TAILWIND_MIGRATION.md`
- **Dependencies:** PR 2

### PR 4: Organizations and admin Tailwind

- **Description:** Convert organizations pages, create modal, onboarding, admin UI domain classes to utilities. Delete orgs/admin/onboarding CSS. Dual-sync. `make check`.
- **Files/components affected:** `examples/fullstack-app/src/app/organizations/**`, `examples/fullstack-app/src/app/admin/**`, `examples/fullstack-app/input.css`, dual-sync, `TAILWIND_MIGRATION.md`
- **Dependencies:** PR 3

### PR 5: Workspace shell Tailwind

- **Description:** Convert workspace chrome (sidebar full/mini/hidden, topbar, org switcher, user menu, pure-CSS mobile drawer, nav icons → inline SVG). Preserve FOUC script and no-JS drawer. Delete workspace shell CSS. Dual-sync. `make check`.
- **Files/components affected:** `examples/fullstack-app/src/app/workspace/mod.rs`, `examples/fullstack-app/input.css`, dual-sync, `TAILWIND_MIGRATION.md`
- **Dependencies:** PR 4

### PR 6: Residual CSS purge and closeout

- **Description:** Grep-delete all remaining semantic CSS; shrink `input.css` to pure Tailwind entry + theme + base. Fix any leftover class strings. Update Playwright smokes to `data-testid`. Final dual-sync + `make check`. Update `TAILWIND_MIGRATION.md` / `HANDOVER.md` status to complete.
- **Files/components affected:** `examples/fullstack-app/input.css`, remaining `src/**/*.rs` with semantic classes, `scripts/verify_*.mjs`, dual-sync, docs
- **Dependencies:** PR 5
