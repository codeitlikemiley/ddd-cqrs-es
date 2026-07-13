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

Mail and optional SpiceDB writes run in `wasi-auth-outbox-worker`; HTTP requests
only commit durable outbox rows. For local development, `make dev` installs the
matching worker version into `target/wasi-auth-tools` and runs it with Spin.
Use `make outbox-worker` and `make spin` in separate terminals only when you
want to inspect their logs independently. Both processes use the same outbox
key/version, while mail and SpiceDB write credentials are supplied only to the
worker. Capture mode does not send internet email: registration and resend
pages expose the locally captured one-time link after the worker delivers it.
To send real mail with Resend, set `AUTH_MAIL_TRANSPORT=resend`,
`AUTH_RESEND_API_KEY`, and `AUTH_RESEND_FROM` in `.env`; the key is passed only
to the native worker and never to Spin.

```dotenv
AUTH_MAIL_TRANSPORT=resend
AUTH_RESEND_API_KEY=re_your_real_key
AUTH_RESEND_FROM="wasi-auth <auth@your-verified-domain.com>"
```

Then run `make dev`. Keep the sender quoted when it contains a display name or
angle brackets. Do not put the API key in `spin.toml`, browser code, or a Spin
variable.
Production rejects capture mail and the documented development key, and
requires distinct ingress, vault, outbox, and recovery-code secrets.

## What the outbox worker does

`wasi-auth-outbox-worker` is not an email server and it does not replace Resend.
It is a small native background process that reliably delivers work already
committed by the application. Registration, password reset, invitation, and
security-notification requests write the user change and an encrypted mail job
to PostgreSQL in one transaction. The HTTP request then returns; the worker
leases the pending job, calls the selected provider, and records `delivered`,
retry, or `dead_letter` status. Optional SpiceDB relationship writes use the
same mechanism.

```text
Spin request -> PostgreSQL user change + pending mail job -> HTTP response
                                      |
                                      v
                           outbox worker -> Resend / HTTP mail provider
                                      |
                                      v
                           delivery ID + final status in PostgreSQL
```

If the worker is stopped, the application still accepts the request but the
mail job remains pending; starting the worker later delivers the backlog. This
keeps provider outages and worker restarts from losing verification messages.
The worker is also where provider credentials belong, so the Resend key never
enters the Spin WASM component.

`make dev` starts both processes and cleans them up together. Run
`make outbox-worker` and `make spin` separately only to inspect logs or deploy
the processes independently. A production deployment runs at least one
worker replica beside the Spin service, sharing PostgreSQL and the outbox key.

The checked-in scripts make the release gates reproducible: run
`benchmark_ingress_overhead.sh` against a direct guest baseline and native
terminal, `benchmark_fullstack.sh` for five absolute concurrency-100 samples,
and `soak_fullstack.sh` for ten-minute status, transport, memory, revocation,
and sensitive-log checks. The paired gate compares the same protected Cedar
operation; anonymous proxy traffic is diagnostic only.
