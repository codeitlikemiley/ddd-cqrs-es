# Fullstack app handover — modularization + Tailwind closeout

**Audience:** external reviewer (e.g. ChatGPT) and human owner.  
**Purpose:** verify modularization + product polish + pure Tailwind UI rewrite.  
**Date:** 2026-07-16  
**Repo:** `/Users/uriah/Code/ddd`  
**Branch:** `execute-plan/710bca15-pr-6-residual-css-purge-and-closeout` (local commits only; **do not push/PR** unless the human asks)

---

## 1. Executive summary

### Production-closure addendum (2026-07-15)

The cross-repository review found runtime and security blockers beyond the
original modularization scope. They were fixed on the matching
`codex/fullstack-verification-flow` branch in this repository and
`/Users/uriah/Code/wasi-auth`:

- rolling `wasi-auth` PostgreSQL migrations now cover organization slugs and
  the fullstack permission families;
- transactional mail snapshots validated product/action configuration into an
  encrypted, versioned outbox payload;
- session finalization binds the default organization before token issuance and
  revokes failed sessions;
- dashboard, resource, query, notification, and vault state now use
  organization-scoped PostgreSQL tables owned by the app;
- live permissions and AAL requirements are enforced for dashboard, query, and
  vault operations, with mutation classification performed on the server;
- the internal application/auth database can no longer be selected as a user
  dashboard connector;
- example-to-CLI-template sync is source-to-template only and drift-checked;
- the invitation island now hydrates from deterministic SSR state; desktop and
  mobile browser smoke pass without hydration panics or horizontal overflow.

Verified after the final release build:

```text
make check                         PASS
make grpc-check                    PASS
bash scripts/check_loc.sh          PASS
make db-verify                     PASS (0011-0013 applied; none pending)
make smoke                         PASS
npm run browser-smoke              PASS (desktop + mobile)
cargo test -p ddd-cqrs-es-cli      PASS (17 tests)
wasi-auth workspace/all-features   PASS (213 passed, 12 ignored)
template sync check                PASS
```

Release dependency: the example intentionally uses the adjacent local
`wasi-auth` checkout while these APIs are unreleased. Generated standalone
projects remove that local patch and must not be advertised as registry-only
buildable until the matching `wasi-auth` release is published and the pinned
version is advanced. Publishing, pushing, and opening a PR remain outside this
handover's authorization.

Work on this branch did two things, in order:

1. **Product / UX fixes** on the authenticated fullstack demo (layout, org selection, vault, dark overlay, agent login helper).
2. **Structural modularization** of `examples/fullstack-app` (and dual-synced CLI template) so large Rust monoliths are split into domain modules under a **1200 LOC** budget.

**Modularization goal is complete** (see `REFACTOR_GOAL.md`).  
**Tailwind rewrite is complete** (see `TAILWIND_MIGRATION.md`) — pure utilities via `src/ui/classes.rs`; residual semantic CSS purged.

**What this is not:** a full product redesign or API redesign. Modularization commits claim *mechanical* moves (split files, re-exports, import fixes) with **no intentional behavior change**. Tailwind commits are UI-only class/CSS rewrites.

---

## 2. How to review (checklist for ChatGPT / reviewer)

### 2.1 Scope gate

Confirm the review stays on:

| In scope | Out of scope (for now) |
|----------|-------------------------|
| File splits, module boundaries, re-exports | Pushing or opening PRs |
| Compile / LOC guards | New product features |
| Dual-sync example ↔ template | Rewriting wasi-auth / ddd_cqrs_es crates |
| Product polish already landed (layout, org, vault UX) | Non-UI backend redesign |
| Pure Tailwind UI rewrite (complete — see tracker) | |

### 2.2 Commands the human can re-run

From `examples/fullstack-app`:

```bash
# HTTP / Leptos shell (default check used by modularization loop)
make check

# gRPC surface (required if reviewing grpc/)
make grpc-check

# LOC budget (max 1200; allowlist currently only mod.rs basenames)
bash scripts/check_loc.sh

# Example → CLI template sync (must stay green after src/ edits)
bash scripts/sync_fullstack_template.sh
# re-run check_loc after sync if needed
```

Dual-sync destination: `crates/ddd-cli/templates/fullstack` (must mirror example product sources for scaffolding).

### 2.3 Review questions

1. **Compile:** Do `make check` and `make grpc-check` both pass on this branch?
2. **LOC:** Is every `src/**/*.rs` ≤ 1200 LOC except intentional `mod.rs` barrel allowlist?
3. **Call sites:** Do external paths still work? (`crate::application::*`, `crate::store::*`, `crate::auth_product::*`, `crate::grpc::{is_grpc_request,serve}`, `crate::app` re-exports for server_fns)
4. **Visibility:** Are `pub` / `pub(crate)` promotions only for cross-module access after splits (not API expansion)?
5. **Behavior:** Any accidental product logic change inside “refactor” commits? (diff by commit; prefer mechanical)
6. **Template drift:** Is `crates/ddd-cli/templates/fullstack` aligned with `examples/fullstack-app` for split modules?
7. **Near-budget files:** Is it OK to leave `app/auth/forms.rs` (~1175) and `app/dashboard/resources.rs` (~1001) for a later thin-out?
8. **Tailwind readiness:** Is the `src/ui/*` + semantic CSS class approach a good base, or should more primitives be extracted first?

### 2.4 Known non-goals / deferred

- **Tailwind:** **complete** — see `TAILWIND_MIGRATION.md`. Markup uses Tailwind utilities via `src/ui/classes.rs`; `input.css` has no residual semantic CSS.
- **Scheduler:** durable 30m modularization loop was **cancelled** after goal completion (`019f615b2152`).
- **Remote:** branch is **ahead of origin by many commits**; nothing was force-pushed by this workstream’s rules.

---

## 3. Product polish (pre-modularization)

Landed earlier on the same branch / conversation stream. Treat as product changes (review for UX correctness, not just structure).

| Area | Intent |
|------|--------|
| Account pages | Max-width / centering (`account-page` ~640px shell) |
| Workspace flyout / create | Create-workspace intent (`?new=1` / force_new behavior) |
| Vault CTA / workspace vault | Workspace-scoped vault UX polish |
| Default org selection | Ensure default org after login/session |
| Org select AAL2 | Owner/admin membership select allowed without erroneous AAL2 gate (wasi-auth SQL / select path) |
| Dashboard wall | Removed “Workspace required” hard wall where inappropriate |
| Dark modal overlay | Overlay scrim uses black-based `--overlay-scrim`, not milky `--bg-inverse` |
| Agent login | `scripts/agent_dev_login.mjs` + docs for automated login in demos |
| Shared UI | `src/ui/*` primitives: button, panel, banner, field, shells, etc. |

**Review focus:** multi-tenancy UX (default org, select org), security of origin/CSRF/session cookie paths (should be unchanged intent), vault/workspace routing.

---

## 4. Modularization architecture

### 4.1 Layering (SSR product)

```
src/lib.rs
├── app/            # Leptos UI islands, routes, server_fns registration surface
├── ui/             # Shared presentational wrappers (pre-Tailwind)
├── contracts/      # Shared DTOs / request-response types
├── application/    # App services used by server_fns, REST, gRPC
├── auth_product/   # Thin adapter over wasi-auth postgres product APIs
├── store/          # KV/SQL/vault/dashboard persistence adapters
├── grpc/           # Spin gRPC services (feature-gated spin-grpc)
├── rest.rs, oauth.rs, server.rs, error.rs
```

**Invariant:** server functions stay registered via **`crate::app` re-exports** (see `server.rs` imports). Islands remain `pub` where the router needs them.

### 4.2 Module map (post-split)

| Former monolith | New tree | Notes |
|-----------------|----------|--------|
| `app` (large) | `app/{router,auth,account,organizations,admin,workspace,dashboard,server_fns,...}` | UI domains |
| `app/account/mod.rs` | `account/{profile,password,mfa,passkeys,sessions,providers,vault}` | |
| `app/auth/mod.rs` | `auth/{pages,forms}` | `forms.rs` still near budget (~1175) |
| `app/dashboard/board.rs` | `dashboard/board/{home,layout,render,util}` | |
| `contracts.rs` | `contracts/{auth,profile,dashboard,vault,resources,organization,admin}` | Barrel re-exports |
| `application.rs` | `application/{request_auth,common,session,auth,profile,dashboard,vault,account,organization,admin,authorization,ingress}` | Cross-module via `pub(crate) use` + paths |
| `store.rs` | `store/{sql,profile,keys,org_slug,board,vault,seed,resources,query_exec,notifications,health}` | Largest risk area historically |
| `auth_product.rs` | `auth_product/{runtime,providers,flows,password,session,organization,admin,infra,config,errors}` | wasi-auth adapter |
| `grpc.rs` | `grpc/{serve,auth,authorization,organization,admin,audit,convert}` + proto modules on barrel | Verify with **`make grpc-check`** |

### 4.3 Split technique (what the agent did)

Mechanical pattern used repeatedly:

1. Cut monolith into domain files by contiguous line ranges / domains.
2. Promote private items to `pub(crate)` when siblings need them after the split.
3. Barrel `mod.rs` with `pub(crate) use domain::*` (or `pub use` for public entry points like gRPC serve).
4. Child modules often `use super::*;` for sibling helpers.
5. Fix orphan trailing `///` docs and attributes (e.g. `#[tonic::async_trait]`) left on the wrong side of a cut.
6. `make check` (and `make grpc-check` for grpc).
7. `bash scripts/sync_fullstack_template.sh`.
8. Update `REFACTOR_GOAL.md` + local commit.

**Risk:** over-broad `pub(crate)` after splits (items that were private in a single file are now crate-visible). Acceptable for internal modularization; reviewer should flag anything that looks like an accidental *public* API expansion outside the crate.

### 4.4 LOC budget

- Guard script: `scripts/check_loc.sh` (`MAX_LOC=1200`).
- Current allowlist: **`mod.rs` only** (basename match — any barrel `mod.rs` is allowlisted by name; product logic should not live only in huge barrels).
- Snapshot of largest files (approx, post-goal):

| LOC | Path |
|-----|------|
| 1175 | `src/app/auth/forms.rs` |
| 1001 | `src/app/dashboard/resources.rs` |
| 940 | `src/app/dashboard/board/render.rs` |
| 939 | `src/store/query_exec.rs` |
| 793 | `src/oauth.rs` *(not split; under budget)* |
| 762 | `src/rest.rs` *(not split; under budget)* |
| 756 | `src/app/mod.rs` |
| 682 | `src/auth_product/infra.rs` |

---

## 5. Dual-sync / template contract

| Source of truth for product demos | Scaffolding template |
|-----------------------------------|----------------------|
| `examples/fullstack-app/` | `crates/ddd-cli/templates/fullstack/` |

Script: `examples/fullstack-app/scripts/sync_fullstack_template.sh`.

**Reviewer must confirm:** splits exist on **both** sides for `application/`, `store/`, `auth_product/`, `grpc/`, `contracts/`, `app/`, `ui/`, and allowlist in `scripts/check_loc.sh` matches.

---

## 6. Git timeline (local modularization series)

Branch tip (as of handover): **`ee2ab3c`** — *ahead of origin by ~19 commits* (count may change).

Representative modularization commits (newest first):

| Commit | Summary |
|--------|---------|
| `ee2ab3c` | chore: modularization loop idle / scheduler cancelled note |
| `a5c8871` | refactor: split **grpc** |
| `d80d9a2` | refactor: split **auth_product** |
| `88f5689` | refactor: split **store** |
| `fdfdd27` | refactor: split **application** |
| `6b80013` | refactor: split **contracts** |
| `9c310b7` | refactor: split dashboard **board** |
| `4825f08` | refactor: split auth UI **pages/forms** |
| `e6e6ce1` | refactor: split **account** UI domains |
| `d92c9aa` | refactor: extract organizations, admin, domain server_fns |
| `4f747f3` / `2e1d908` | refactor: extract auth/account/server_fns; router/workspace/dashboard |
| `2db9d8d` | refactor: shared **UI primitives** and page shells |

Earlier commits on the same branch include workspace vault, default org, agent login, Makefile/spin env alignment — product features, not pure refactors.

**Suggested git review commands:**

```bash
git log --oneline origin/codex/fullstack-verification-flow..HEAD
git diff origin/codex/fullstack-verification-flow...HEAD --stat
# sample mechanical split
git show fdfdd27 --stat
git show 88f5689 --stat
git show a5c8871 --stat
```

---

## 7. Verification status (last agent run)

| Check | Status |
|-------|--------|
| `make check` | Green after final splits |
| `make grpc-check` | Green after grpc split |
| `scripts/check_loc.sh` | OK (`allowlist: mod.rs`) |
| Dual-sync | Run after each split unit |
| Push / PR | **Not done** (by design) |
| Tailwind rewrite | **Complete** (see `TAILWIND_MIGRATION.md`) |

**Not claimed:** full browser E2E suite re-run on every modularization commit. Product smoke scripts exist under `scripts/` (`verify_fullstack.sh`, Playwright helpers, vault smoke, etc.) — human should re-run smoke if security-sensitive paths are in doubt.

---

## 8. Tailwind rewrite status

**Status: complete** (Phase 5 closeout). Tracker: `TAILWIND_MIGRATION.md`.  
**Visual system / elevation:** `DESIGN.md` (zinc-25 light canvas, semantic surfaces).

What landed:

1. Design tokens in `@theme inline` + CSS vars (`:root` / dark) — light canvas ≈ zinc-25 so pure-white surfaces float (see `DESIGN.md`).
2. Shared utilities in `src/ui/classes.rs`; primitives (`button`, `panel`, shells) emit utilities.
3. Domain markup converted (auth, account, board, settings, orgs, admin, workspace shell, settings shell, home).
4. Residual semantic CSS purged from `input.css` (tiny non-semantic residuals only: drawer/modal scroll locks + `board-pulse` keyframes).
5. Playwright smokes prefer `data-testid` over class names.

Post-Tailwind hygiene (optional, not blockers):

- Split `app/auth/forms.rs` further if it grows.
- Split `app/dashboard/resources.rs` further if it grows.
- Optionally modularize `oauth.rs` / `rest.rs` if they grow.

---

## 9. Security / multi-tenant notes for reviewers

These areas deserve **behavior-level** scrutiny (not just file layout):

1. **Org selection / default org** — ensure users cannot select orgs they are not members of; default-org path must not escalate privilege.
2. **AAL2 / step-up** — admin and sensitive vault actions still require step-up where product requires it; membership select for owner/admin should not incorrectly require AAL2 if that was the intentional fix.
3. **Vault** — org-scoped secrets; reveal path; encryption helpers in `store/vault`.
4. **Browser origin validation** — `application/ingress` loopback alias matching for local dev.
5. **gRPC auth** — `request_auth` metadata → `RequestAuth` mapping in `grpc/convert` / services.
6. **Session cookies** — secure flag, Host-session naming via application session helpers.

If any of the above look diluted by a “refactor” or UI commit, flag as **blocker**.

---

## 10. Open questions for the human / ChatGPT

1. Accept modularization + Tailwind closeout as **done** for product UI gate?
2. Require re-run of full smoke/browser suite on this branch tip first?
3. Thin `forms.rs` / `resources.rs` now or later?
4. Push branch / open PR for CI?
5. Keep CLI template dual-sync strict 1:1 going forward?

---

## 11. Key file index

| Path | Role |
|------|------|
| `examples/fullstack-app/REFACTOR_GOAL.md` | Durable modularization goal + progress log |
| `examples/fullstack-app/HANDOVER.md` | This document |
| `examples/fullstack-app/scripts/check_loc.sh` | LOC budget guard |
| `examples/fullstack-app/scripts/sync_fullstack_template.sh` | Dual-sync to CLI template |
| `examples/fullstack-app/Makefile` | `check`, `grpc-check`, spin targets |
| `examples/fullstack-app/src/ui/` | Tailwind utility constants + UI primitives |
| `examples/fullstack-app/TAILWIND_MIGRATION.md` | Tailwind rewrite tracker (complete) |
| `examples/fullstack-app/DESIGN.md` | Visual system brief (zinc-25 light elevation, semantic tokens) |
| `examples/fullstack-app/input.css` | Pure Tailwind v4 entry (tokens + base) |
| `examples/fullstack-app/src/app/` | Routes, islands, server_fns |
| `examples/fullstack-app/src/application/` | App service layer |
| `examples/fullstack-app/src/store/` | Persistence |
| `examples/fullstack-app/src/auth_product/` | wasi-auth product adapter |
| `examples/fullstack-app/src/grpc/` | gRPC services |
| `crates/ddd-cli/templates/fullstack/` | Scaffold mirror |

---

## 12. Suggested ChatGPT review prompt (copy-paste)

```text
You are reviewing a Rust/Leptos fullstack modularization branch before a Tailwind rewrite.

Repo context: ddd monorepo, product at examples/fullstack-app, dual-synced to
crates/ddd-cli/templates/fullstack. Branch: codex/fullstack-verification-flow.

Read examples/fullstack-app/HANDOVER.md and REFACTOR_GOAL.md first.

Tasks:
1. Assess whether modularization is complete and safe (LOC, module boundaries,
   re-exports, dual-sync, no accidental API/behavior breaks).
2. Call out any blockers before Tailwind.
3. Rank residual risks (store, auth_product, grpc, org/vault security).
4. Recommend whether to thin forms.rs/resources.rs now or during Tailwind.
5. Propose a minimal verification plan (commands + smoke) for the human.
6. Do NOT propose starting Tailwind until you explicitly recommend go/no-go.

Be skeptical of “mechanical only” claims—spot-check likely risky diffs.
```

---

## 13. Handover status

| Item | State |
|------|--------|
| Modularization goal | **Complete** |
| LOC budget | **OK** |
| Tailwind rewrite | **Complete** (see `TAILWIND_MIGRATION.md`) |
| Durable agent loop | **Cancelled** |
| Push/PR | **Not done** |

**Owner next step:** re-run smoke suite if desired; push/PR only when explicitly authorized.
