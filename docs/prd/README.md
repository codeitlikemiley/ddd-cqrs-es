# Auth Stack PRD Tracker

This directory tracks the planned `ddd-auth` and `ddd-authz` work before
implementation. These documents are the source of truth for scope, interfaces,
milestones, and verification while the auth stack is designed and built.

## Status Legend

- `planned`: approved for design tracking, not implemented.
- `in_progress`: active implementation work has started.
- `implemented`: merged and verified against the PRD checks, with any external
  operator-only gates documented in the owning rollout guide.
- `superseded`: replaced by a newer PRD or design decision.

## Roadmap

| PRD | Status | Owner | Purpose |
| --- | --- | --- | --- |
| [Auth implementation tracker](./auth-implementation-tracker.md) | implemented | Coordinating agent | Durable status, milestones, expected files, and verification evidence. |
| [Auth surface contracts](./auth-surface-contracts.md) | implemented | Fullstack agent | Defines routes, pages, forms, server functions, REST endpoints, and gRPC methods as separate contracts. |
| [ddd-auth](./ddd-auth.md) | implemented | Auth crate agent | Identity, sessions, JWT/JWKS, OAuth/OIDC, passkeys, and token lifecycle. |
| [ddd-authz](./ddd-authz.md) | implemented | Authz crate agent | RBAC, ReBAC, ABAC, relationship tuples, models, and checks. |
| [Spin auth stack](./spin-auth-stack.md) | implemented | Runtime agent | Spin components, service boundaries, REST/gRPC, and internal service calls. |
| [Leptos auth UI](./leptos-auth-ui.md) | implemented | UI agent | Login, passkeys, OAuth redirects, auth-required/forbidden pages, admin screens, and redirect UX. |
| [CLI auth templates](./cli-auth-templates.md) | implemented | CLI agent | `ddd` capabilities, presets, templates, and dry-run support. |
| [Auth storage and migrations](./auth-storage-and-migrations.md) | implemented | Storage agent | Event-sourced aggregates, projections, schema bootstrap, and migrations. |
| [Auth verification and rollout](./auth-verification-rollout.md) | implemented | Verification agent | Compile matrix, REST/gRPC smoke checks, UI checks, and release gates. |
| [Auth production hardening plan](./auth-production-hardening-plan.md) | in_progress | Security hardening agent | Integrated post-A36 production security plan (A37–A48): surface enforcement, secrets hygiene, CSRF, KDF, OAuth PKCE, tenant fail-closed, abuse limits, and security smoke. |

## Milestone Order

1. Run dependency spikes for JWT, JWK, OAuth/OIDC, WebAuthn/passkeys, CBOR, COSE,
   and crypto crates against `wasm32-wasip2` and Spin.
2. Lock the auth surface contracts so routes, pages, forms, server functions,
   REST endpoints, and gRPC methods are not conflated during implementation.
3. Implement reusable `ddd-auth` and `ddd-authz` crates behind feature flags.
4. Build the Spin runtime example using shared application services and thin
   REST, gRPC, and Leptos boundaries.
5. Add CLI generation support for auth-enabled projects.
6. Add verification scripts, smoke tests, and docs updates before marking any
   PRD `implemented`.
7. Execute [Auth production hardening plan](./auth-production-hardening-plan.md)
   (A37–A48) before treating the auth-stack example as production-grade.

## Update Rules

- Update the status table whenever implementation starts, lands, or is replaced.
- Keep implementation decisions in the focused PRD that owns the subsystem.
- Keep code changes out of PRD-only commits unless the commit is explicitly a
  follow-up implementation task.
- If a dependency spike fails for WASI or Spin, update the owning PRD with the
  selected fallback before implementation continues.
