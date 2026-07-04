---
title: CLI Auth Templates PRD
description: Plan ddd CLI support for generating auth-enabled Spin and Leptos WASI applications.
---

# CLI Auth Templates PRD

## Status

implemented

## Goal

Extend the `ddd` CLI so agents and developers can generate reproducible
auth-enabled Spin applications using dry-run JSON, `ddd.toml`, and the existing
runtime/backend/transport vocabulary.

## Non-Goals

- Do not remove or rename existing presets.
- Do not add unsupported runtime values.
- Do not make generated apps depend on secrets being present at generation time.
- Do not generate provider-specific secrets into source files.

## Success Criteria

- `ddd capabilities --json` advertises auth and authz capabilities.
- `ddd init ... --preset auth-stack` plans and writes a Spin auth stack project.
- Generated `auth-stack` projects default to fullstack support:
  `--transport both --ui leptos`.
- Dry-run JSON lists every file operation before writing.
- `ddd.toml` records auth capabilities, selected providers, UI mode, transport,
  database, and realtime mode.
- Generated projects pass `ddd check`.

## Interfaces

### CLI Surface

New preset:

```bash
ddd init auth-stack --preset auth-stack --runtime spin --db sqlite --transport both --ui leptos
```

If the user omits `--transport` or `--ui` for `preset=auth-stack`, default to
`transport=both` and `ui=leptos` so the generated project includes web pages,
REST, and gRPC by default.

New capabilities:

```bash
ddd enable auth
ddd enable authz
ddd enable passkeys
ddd enable oauth-provider google
ddd enable oauth-provider apple
ddd enable oauth-provider facebook
```

Provider enablement records provider IDs and placeholder secret references in
`ddd.toml`. It does not write client secrets.

### Manifest Additions

`ddd.toml` should support:

- `capabilities.enabled = ["auth", "authz", "passkeys", "oauth:google"]`
- `[auth]` for issuer, audience, token TTLs, refresh TTLs, and cookie mode.
- `[[auth.providers]]` for provider ID, issuer, scopes, client ID env name, and
  client secret reference.
- `[authz]` for active model selector and default decision policy.

### Template Files

The CLI template generates:

- `Cargo.toml`
- `Makefile`
- `spin.toml`
- `src/application.rs`
- `src/rest.rs`
- `src/grpc.rs`
- `src/server.rs`
- `src/app.rs`
- `proto/auth.proto`
- `proto/authz.proto`
- `ddd.toml`
- focused README and `.env.example`

## Implementation Milestones

1. Add `auth-stack` to the CLI model, capabilities JSON, matrix validation, and
   tests.
   - Status: done. `Preset::AuthStack` is advertised by `ddd capabilities
     --json`, defaults to `transport=both` and `ui=leptos`, and rejects
     non-fullstack transport/UI selections.
2. Add dry-run template rendering with placeholders and no secret values.
   - Status: done. Dry-run JSON lists `spin.toml`,
     `spin.production.toml.example`, `.env.example`, `build.rs`, `input.css`,
     `package.json`, runtime `src/*.rs`, `proto/*.proto`, and smoke/rollout
     scripts before writing. Generated `.env.example`, `Makefile`, `spin.toml`,
     and `spin.production.toml.example` include provider endpoint, JWKS, scope,
     redirect URI, signing-key key-ring, admin-token, and credential variable
     names without embedding secret values. Generated passkey settings include
     RP ID, RP name, origin, user-verification policy, base64 strictness,
     authenticator attachment, and challenge TTL variables. Generated
     storage/session settings include `DATABASE_BACKEND`, `POSTGRES_URL`,
     `MYSQL_URL`, `AUTH_STORAGE_AUTO_CATCH_UP`, and `AUTH_COOKIE_SECURE`.
     Generated Makefiles include `oauth-credentials`, `oauth-preflight`,
     `oauth-evidence`, `oauth-dev-browser-smoke`, `oauth-browser-smoke`,
     `oauth-callback`, `browser-smoke`, and `passkey-browser-smoke` so provider
     credentials, local OAuth UI redirects, live authorization redirects,
     redacted event evidence, interactive browser login, callback evidence,
     page middleware, and WebAuthn behavior can be checked before A11 is closed.
     Generated Makefiles and Spin manifests pass the declared JWT, OAuth,
     passkey, password, admin-token, cookie, and public-base-url variables into
     runtime components instead of leaving them as inert global defaults.
3. Add `ddd enable auth`, `authz`, `passkeys`, and `oauth-provider`.
   - Status: done. OAuth provider enablement writes provider IDs and env-var
     names such as `AUTH_GOOGLE_ENABLED`, `AUTH_GOOGLE_CLIENT_ID`, and
     `AUTH_GOOGLE_CLIENT_SECRET`, not credential values.
4. Add generated project checks for REST, gRPC, Leptos, and storage features.
   - Status: done. `ddd check` now requires the auth-stack Leptos, REST, gRPC,
     OAuth, storage, proto, package, and script files for `preset=auth-stack`
     and no longer requires the unrelated aggregate-domain scaffold.
     Regression tests prove generated `Cargo.toml`, `Makefile`, `spin.toml`,
     `.env.example`, scripts, and package files carry Spin PostgreSQL/MySQL
     feature hooks, backend runtime variables, smoke targets, and package/crate
     substitutions. Source-checkout CLI runs also emit local
     `[patch.crates-io]` overrides for `ddd_cqrs_es`, `ddd-auth`, and
     `ddd-authz` so generated projects compile before the auth crates are
     published; published CLI builds omit those local patches and use the
     declared crates.io versions.
5. Update CLI docs after generated output is stable.
   - Status: done. `docs/cli.md` documents the `auth-stack` preset, manifest
     auth/authz sections, and auth enable commands.

## Verification

- `cargo test -p ddd-cqrs-es-cli --all-targets`.
- `cargo run -p ddd-cqrs-es-cli -- capabilities --json`.
- `cargo run -p ddd-cqrs-es-cli -- --dry-run --format json init /tmp/ddd-auth-stack-dry-run --preset auth-stack --runtime spin --db sqlite --transport both --ui leptos`.
- Full write test proving generated files exist and `ddd check` passes.
- Full write test proving generated auth-stack projects include real runtime
  files, scripts, package metadata, no unused aggregate scaffold, and pass
  `ddd check`.
- Generated auth-stack compile probe:
  `cargo check --manifest-path /tmp/ddd-auth-stack-compile-probe/Cargo.toml
  --no-default-features --features ssr,sqlite` and
  `cargo check --manifest-path /tmp/ddd-auth-stack-compile-probe/Cargo.toml
  --target wasm32-wasip2 --no-default-features --features ssr,sqlite,spin-grpc`.
- Capabilities JSON test proving auth capabilities and templates are listed.
- Regression test proving unsupported runtime values are still rejected and
  `preset=auth-stack` rejects non-fullstack transport/UI selections.
