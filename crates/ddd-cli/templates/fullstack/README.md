# Fullstack Template

This template entry is advertised by `ddd capabilities --json` for projects
created with:

```bash
ddd init fullstack-app --preset fullstack --runtime spin --db postgres --transport both --ui leptos
```

The renderer lives in `crates/ddd-cli/src/render.rs` so generated projects can
derive names, manifests, and feature flags from CLI arguments. Generated
projects include `spin.production.toml.example` as the exact-host production
manifest starting point.

The generated project requires Rust 1.93.0+, `cargo-leptos >= 0.3.7`, the
distributed `wasm32-wasip2` Rust target, and `wasm-tools`. The Rust target supplies `std`;
artifact verification proves that the component itself exports
`wasi:http/handler@0.3.0` and retains no Preview 1 imports. The unstable
`wasm32-wasip3` Rust target remains a canary until its self-contained libraries
are distributed.

`wasi-auth` owns the only authentication schema. The generated PostgreSQL
profile applies it with the advisory-lock-protected migration runner before
Spin starts. Production traffic terminates at the signed native ingress while
the final-WASI Spin backend remains loopback-only; migration
`0009_context_invalidation` is required for notification-backed revocation.
Migration `0010_typed_relationship_outbox` atomically emits resource-scoped
SpiceDB intents from membership changes.

Mail and optional SpiceDB writes run in the separately installed
`wasi-auth-outbox-worker`; HTTP requests only commit durable outbox rows. Start
`make outbox-worker` and `make spin` in separate terminals. Both processes use
the same outbox key/version, while mail and SpiceDB write credentials are
supplied only to the worker. Production rejects the documented development key
and requires distinct ingress, vault, outbox, and recovery-code secrets.

The checked-in scripts make the release gates reproducible: run
`benchmark_ingress_overhead.sh` against a direct guest baseline and native
terminal, `benchmark_fullstack.sh` for five absolute concurrency-100 samples,
and `soak_fullstack.sh` for ten-minute status, transport, memory, revocation,
and sensitive-log checks. The paired gate compares the same protected Cedar
operation; anonymous proxy traffic is diagnostic only.
