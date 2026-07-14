# Fullstack modularization goal (pre-Tailwind)

**Status:** complete  
**Branch:** `codex/fullstack-verification-flow`  
**Rule:** do **not** push or open PRs unless the human explicitly asks.

## Goal

Finish structural cleanup of `examples/fullstack-app` (and dual-sync template) so every product Rust file is a manageable size, routes stay scalable, and shared UI lives in `src/ui/`. **Do not** start full Tailwind utility rewrite until this goal is complete.

### Done when

1. No `src/**/*.rs` file above **1200 LOC** without a documented temporary allowlist entry that is still shrinking.
2. UI tree is modular under `src/app/{router,auth,account,organizations,admin,workspace,dashboard,server_fns,helpers,path}` (already largely true).
3. Remaining large modules are split:
   - [x] `src/app/account/mod.rs` (~2.4k) → `profile`, `password`, `mfa`, `passkeys`, `sessions`, `providers`, `vault`
   - [x] `src/app/auth/mod.rs` (~1.4k) → `pages` + `forms`
   - [x] `src/app/dashboard/board.rs` (~1.5k) → `board/{home,layout,render,util}`
   - [x] `src/contracts.rs` (~1.7k) → `contracts/{auth,profile,dashboard,vault,resources,organization,admin}`
   - [x] `src/application.rs` (~2.9k) → `application/{mod,request_auth,common,session,auth,profile,dashboard,vault,account,organization,admin,authorization,ingress}`
   - [x] `src/store.rs` (~4.5k) → `store/{sql,profile,keys,org_slug,board,vault,seed,resources,query_exec,notifications,health}`
   - [x] `src/auth_product.rs` (~2.3k) → `auth_product/{runtime,providers,flows,password,session,organization,admin,infra,config,errors}`
   - [x] `src/grpc.rs` (~1.5k) → `grpc/{serve,auth,authorization,organization,admin,audit,convert}`
4. Every change: `make check` green in `examples/fullstack-app` (and `make grpc-check` when touching gRPC).
5. Dual-sync: `bash scripts/sync_fullstack_template.sh` so `crates/ddd-cli/templates/fullstack` matches.
6. Local commits only (group by task). No force-push, no remote push.

### Constraints

- Mechanical moves preferred; no product/behavior changes.
- Keep semantic CSS class names; `src/ui/*` wrappers stay until Tailwind phase.
- Islands must remain `pub` where router/server_fns need them.
- Server functions stay registered via `crate::app` re-exports (`server.rs` imports).

### Next work unit (update after each run)

**Goal complete.** Optional follow-ups (out of scope for this goal unless reopened):
1. Thin remaining near-budget files further if desired (e.g. `app/auth/forms.rs` ~1175).
2. Tailwind utility rewrite (explicitly deferred until after this goal).
3. Cancel the durable 30m scheduler if no further modularization units remain.

LOC allowlist is now only `mod.rs` (barrel modules; not a monolith).

### Agent loop

Durable scheduled job every **30m** reads this file, does **one** work unit, updates this file, commits locally. No push/PR.
Session goal tracks the same objective via `/goal` / `update_goal`.

### Progress log

- 2026-07-14: UI shells/primitives; app tree; auth/account/server_fns; orgs/admin; domain server_fns. `app/mod.rs` ~756 LOC.
- 2026-07-14 (scheduled): Split `app/account/` into profile, password, mfa, passkeys, sessions, providers, vault. All files &lt;700 LOC. `make check` green.
- 2026-07-14 (scheduled): Split `app/auth/` into `pages.rs` (~229) + `forms.rs` (~1175). `make check` green.
- 2026-07-14 (scheduled): Split dashboard board into `board/{home,layout,render,util}` (all &lt;1000 LOC). Removed board/resources from LOC allowlist. `make check` green.
- 2026-07-14 (scheduled): Split `contracts` into domain modules (auth, profile, dashboard, vault, resources, organization, admin) with barrel re-exports. Removed contracts from LOC allowlist. `make check` green.
- 2026-07-14 (scheduled): Split `application` into domain modules (~51–511 LOC each); fixed cross-module imports/helpers; removed `application.rs` from LOC allowlist. `make check` green; dual-sync done.
- 2026-07-15 (scheduled): Split `store` into domain modules (~106–937 LOC each); removed `store.rs` from LOC allowlist. `make check` green; dual-sync done.
- 2026-07-15 (scheduled): Split `auth_product` into domain modules (~148–682 LOC each); removed `auth_product.rs` from LOC allowlist. `make check` green; dual-sync done.
- 2026-07-15 (scheduled): Split `grpc` into domain modules (~64–494 LOC each); `make check` + `make grpc-check` green; removed `grpc.rs` from LOC allowlist; dual-sync done. **Modularization goal complete.**
- 2026-07-15 (scheduled): No-op — goal already complete; LOC budget OK (only near-budget: `app/auth/forms.rs` ~1175, `app/dashboard/resources.rs` ~1001). Cancelled durable 30m scheduler `019f615b2152`.

Update this file every scheduled run: checkboxes, progress log line, and the single “Next work unit”.
