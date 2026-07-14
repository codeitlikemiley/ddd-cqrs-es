# Fullstack modularization goal (pre-Tailwind)

**Status:** in progress  
**Branch:** `codex/fullstack-verification-flow`  
**Rule:** do **not** push or open PRs unless the human explicitly asks.

## Goal

Finish structural cleanup of `examples/fullstack-app` (and dual-sync template) so every product Rust file is a manageable size, routes stay scalable, and shared UI lives in `src/ui/`. **Do not** start full Tailwind utility rewrite until this goal is complete.

### Done when

1. No `src/**/*.rs` file above **1200 LOC** without a documented temporary allowlist entry that is still shrinking.
2. UI tree is modular under `src/app/{router,auth,account,organizations,admin,workspace,dashboard,server_fns,helpers,path}` (already largely true).
3. Remaining large modules are split:
   - [x] `src/app/account/mod.rs` (~2.4k) → `profile`, `password`, `mfa`, `passkeys`, `sessions`, `providers`, `vault`
   - [ ] `src/app/auth/mod.rs` (~1.4k) → `pages` + `forms` (or similar)
   - [ ] `src/app/dashboard/board.rs` (~1.5k) → tiles/modals/metrics as needed
   - [ ] `src/store.rs` (~4.5k) → `store/{profile,vault,board,query_exec,org_slug,health,...}`
   - [ ] `src/application.rs` (~2.9k) → domain modules
   - [ ] `src/auth_product.rs` (~2.3k) → domain modules if still large
   - [ ] `src/contracts.rs` (~1.7k) → domain modules
4. Every change: `make check` green in `examples/fullstack-app`.
5. Dual-sync: `bash scripts/sync_fullstack_template.sh` so `crates/ddd-cli/templates/fullstack` matches.
6. Local commits only (group by task). No force-push, no remote push.

### Constraints

- Mechanical moves preferred; no product/behavior changes.
- Keep semantic CSS class names; `src/ui/*` wrappers stay until Tailwind phase.
- Islands must remain `pub` where router/server_fns need them.
- Server functions stay registered via `crate::app` re-exports (`server.rs` imports).

### Next work unit (update after each run)

1. Split `app/auth/mod.rs` into `pages` + `forms` (or equivalent).
2. Then `app/dashboard/board.rs`.
3. Then backend: `contracts` → `application` → `store`.

### Progress log

- 2026-07-14: UI shells/primitives; app tree; auth/account/server_fns; orgs/admin; domain server_fns. `app/mod.rs` ~756 LOC.
- 2026-07-14 (scheduled): Split `app/account/` into profile, password, mfa, passkeys, sessions, providers, vault. All files &lt;700 LOC. `make check` green.

Update this file every scheduled run: checkboxes, progress log line, and the single “Next work unit”.
