# Contributing Guide

## Environment and command wrapper

Repository AGENTS guidance requires workspace commands to be prefixed with `rtk`.
Use `rtk` for Makefile and command-line execution in this repo context.

## Core development workflow

- Start from `examples/counter-app` for runtime-focused changes.
- Use the Makefile as the backend command source of truth:
  - `rtk make help`
  - `rtk make help-db`
  - `rtk make help-realtime`
  - `rtk make help-matrix`
  - `rtk make help-env`

## Backend contract defaults

- Supported public backends are: `sqlite`, `postgres`, `neon`, `supabase`, `turso`, `mysql`, `redis`.
- `db=turso` is the supported public value for Turso/LibSQL.
- `libsql` is retained as an internal compatibility path in runtime/reset internals and should not be documented as a public `make db=<...>` option.
- Realtime modes: `off`, `polling`, `redis`.

## Reset semantics (important)

- `make db=<backend> fresh` is reset-only. It drops/recreates backend state and exits.
- `fresh` must never launch the app server.

## Required edits for backend/realtime changes

When updating backend or realtime behavior, keep docs in sync in all of these locations:

- `examples/counter-app/Makefile`
- `examples/counter-app/README.md`
- `examples/counter-app/.env.example`
- `docs/tutorial/leptos-ssr.md`
- `docs/production/redis.md`
- `.agents/skills/leptos-wasi-cqrs/SKILL.md`

When updating the `ddd` CLI command surface, generated templates, runtime matrix, or release behavior, keep these in sync:

- `docs/cli.md`
- `SKILLS.md`
- `.agents/skills/ddd-cli/SKILL.md`
- `crates/ddd-cli/tests/cli.rs`

## Documentation quality checks

Run this before docs-focused PRs:

- `rtk node -v` (environment sanity check, if needed)
- `rtk jq -r '.navigation.groups[].pages[]' docs/docs.json | sort` and compare against `docs/**/*.md`
- `rtk scripts/verify-docs.sh`

## Release process (crates.io)

For a release to [crates.io](https://crates.io), publish the library and CLI as a same-version pair:

- Dry run:
  - `rtk make publish dry-run` or `rtk make publish -- --dry-run`
- Publish:
  - `CARGO_REGISTRY_TOKEN=<token> rtk make publish`

The release script validates and publishes both packages in order:

- `ddd_cqrs_es`
- `ddd-cli` (binary name: `ddd`)

## Versioning and example shortcuts

- Bump both workspace package versions:
  - `make version` (auto-increment patch)
  - `make version 0.2.1` (explicit version)
- Run example with same pattern as app Makefile:
  - `make example spin db=neon realtime=redis`
