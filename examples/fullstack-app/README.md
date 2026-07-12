# fullstack-app

Generated with `ddd init --preset fullstack`.

This project is a Spin fullstack authentication and authorization service with Leptos pages, REST endpoints, and gRPC service contracts.

- Runtime: `spin`
- DB: `postgres`
- Transport: `both`
- UI: `leptos`
- Auth: email/password enabled by default
- OAuth and passkeys: feature-flagged until credentials are configured

Start with `.env.example`, then run:

```bash
make db-up
make spin
make smoke
make browser-smoke
```

The toolchain gate requires `cargo-leptos >= 0.3.7`, `wasm32-wasip2`, and `wasm-tools`. The distributed P2 Rust target supplies `std`; the generated component is inspected to prove it exports `wasi:http/handler@0.3.0` and has no Preview 1 imports. The unstable `wasm32-wasip3` Rust target remains a canary.

`wasi-auth` owns the only PostgreSQL auth schema. `make db-migrate` uses its native, advisory-lock-protected migration runner before Spin starts; the WASM request component never mutates schema. `make fresh` resets PostgreSQL and reapplies the immutable migration catalog.

For production, start from `spin.production.toml.example`, replace the example auth domain and database hosts with exact deployment hosts, and run the same migration binary as an explicit deployment step.

After OAuth provider credentials and callback URLs are configured, run `make oauth-preflight` before the browser callback smoke. Use `make oauth-browser-smoke` to complete the provider login in a browser, or `make oauth-callback` with an issued session cookie to capture final callback evidence manually.

Use `ddd enable oauth-provider google`, `apple`, or `facebook` to record provider placeholders in `ddd.toml` without writing secrets.
