# Changelog

## 0.3.0-rc.5

- Fullstack settings islands take **route `slug` props** so soft-nav no longer
  hits empty-slug 500s after client-side hops.
- CI: read publishable package version dynamically; normalize monorepo-only
  fullstack example artifacts during drift checks.
- CLI ships the dual-synced product README for `ddd init --preset fullstack`.
- **Note:** `wasi-auth` stays at `0.1.0-rc.2` (no auth crate changes in this
  release).

## 0.3.0-rc.4

- Fix CLI packaging: ship fullstack manifest as `Cargo.toml.template` so
  `cargo package` includes the full template tree (nested `Cargo.toml` was
  treated as a separate package and dropped from the crate).
- Yank recommendation: `ddd-cqrs-es-cli 0.3.0-rc.3` cannot scaffold fullstack
  (`fullstack Cargo.toml must be embedded`); use `0.3.0-rc.4` or later.

## 0.3.0-rc.3

- Fullstack Leptos template: **persistent workspace chrome** soft-nav so org
  switcher, account menu, and theme stay mounted across in-app hops
  (`islands_router` + content-only region swap).
- Cache-first chrome snapshot and client-side flyout focus for settings/account
  menus; composable skeleton loaders for page bodies.
- Documented the technique in
  `docs/tutorial/leptos-islands-persistent-chrome.md` for reuse in other islands
  apps.
- Dual-synced the CLI `fullstack` template with the example product tree.
- **Note:** `wasi-auth` stays at `0.1.0-rc.2` (no auth crate changes in this
  release).

## 0.3.0-rc.2

- Added native Resend delivery through `wasi-auth-outbox-worker` with durable
  delivery status, provider idempotency, and secret-isolated worker startup.
- Clarified the outbox worker, capture mail, Resend configuration, and local
  versus production process topology in the fullstack documentation.
- Fixed verification-page navigation, capture-link UX, stale-cookie handling,
  and browser smoke expectations for ordinary users versus system admins.

## 0.3.0-rc.1

- Removed the duplicate `ddd-auth` and `ddd-authz` products in favor of the
  single `wasi-auth` dependency while keeping the DDD core identity-agnostic.
- Replaced the `auth-stack` CLI preset with the canonical `fullstack` preset and
  byte-for-byte generated `examples/fullstack-app`.
- Added final-WASI Leptos islands, REST, and Spin gRPC dispatch to the fullstack
  example, including bounded audit streaming and production configuration
  guards.
- Added unary, server-streaming, client-streaming, and bidirectional-streaming
  gRPC to the counter example, with an optional authorization feature.
- Hardened atomic idempotent execution so completed retries return their
  original result before an already-applied command is evaluated again.
- Pinned final `wasip3` 0.7.0 and the maintained Spin SDK revision required by
  the generated final-WASI examples.
- Replaced production RS256 access-token signing with ES256 and added a
  public-only JWKS round-trip test; provider-issued RS256 ID tokens remain
  verification-only behind `wasi-auth`'s private-RSA signing denial.

## 0.2.0

- Removed the legacy `store` module shim; use top-level exports or the `event_store` and `memory` modules directly.
- Removed `Aggregate::id()` from the aggregate trait and renamed raw test replay to `replay_raw_events_from_zero`.
- Changed `EventType` from a `String` alias to a serde-transparent newtype.
- Made `SqlSchemaConfig` table names private and added eager validation through fallible builders.
- Added bounded idempotency waits through `IdempotencyWaitConfig` and timeout errors.
- Added process-manager runners for sync and async command dispatch.
- Added `ProjectionRunnerError` `Display` and `Error` implementations.
- Added configurable event-store contract-test sequence expectations.
- Optimized `execute_returning_state` to avoid a second stream load.
- Added bounded global replay and projection batch APIs for production catch-up loops.
- Added schema migration v6 to remove legacy duplicate stream indexes while preserving unique stream constraints.
- Added query-plan coverage for SQLite and live-gated PostgreSQL/MySQL adapter checks.
