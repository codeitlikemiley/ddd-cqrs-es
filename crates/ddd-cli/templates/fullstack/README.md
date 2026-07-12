# Fullstack Template

This template entry is advertised by `ddd capabilities --json` for projects
created with:

```bash
ddd init fullstack-app --preset fullstack --runtime spin --db sqlite --transport both --ui leptos
```

The renderer lives in `crates/ddd-cli/src/render.rs` so generated projects can
derive names, manifests, and feature flags from CLI arguments. Generated
projects include `spin.production.toml.example` as the exact-host production
manifest starting point.

The generated project requires `cargo-leptos >= 0.3.7`, the distributed
`wasm32-wasip2` Rust target, and `wasm-tools`. The Rust target supplies `std`;
artifact verification proves that the component itself exports
`wasi:http/handler@0.3.0` and retains no Preview 1 imports. The unstable
`wasm32-wasip3` Rust target remains a canary until its self-contained libraries
are distributed.

`wasi-auth` owns the only authentication schema. Startup executes its embedded,
checksum-verified migration for either Spin SQLite or PostgreSQL. `make fresh`
only erases application data; it intentionally does not carry a second schema
copy, and the canonical migration is reapplied on the next startup.
