---
name: ddd-cli
description: Use when scaffolding, extending, validating, serving, watching, or publishing ddd_cqrs_es applications through the `ddd` CLI; especially for agent/MCP workflows that need dry-run JSON, `ddd.toml` manifests, fine-grained `ddd add ...` commands, capability enablement, Spin runtime presets, fullstack SaaS scaffolding, or same-version library/CLI release handling.
---

# ddd CLI Skill

## Core Rule

Prefer the `ddd` CLI over hand-written scaffolding when the target is a generated `ddd_cqrs_es` app or a new app created from this repository's templates.

In this repository, run the development binary as:

```bash
cargo run -p ddd-cqrs-es-cli -- <args>
```

After publishing or installing from crates.io, the user-facing command is:

```bash
ddd <args>
```

Current library/CLI version pair: read `version` from root `Cargo.toml` (e.g. `0.3.0-rc.6`). Always keep library and CLI at the **same** version.

## First Moves

Before generating or mutating code:

1. Discover the live command surface:
   ```bash
   cargo run -p ddd-cqrs-es-cli -- capabilities --json
   ```
2. For runtime/backend combinations, inspect:
   ```bash
   cargo run -p ddd-cqrs-es-cli -- matrix
   ```
3. For any mutating command, preview first with dry-run JSON:
   ```bash
   cargo run -p ddd-cqrs-es-cli -- --dry-run --format json <command>
   ```
4. Inspect the planned `operations` before running the same command without `--dry-run`.

The JSON report is the agent/MCP contract. Expect fields like `status`, `message`, `operations`, `command`, and `data`.

## Presets

Supported presets (from `capabilities --json`): `basic`, `leptos-wasi`, **`fullstack`**, `native-api`, `worker`, `custom`.

| Preset | Use for | Domain codegen (`ddd add`) | Runtime after init |
| --- | --- | --- | --- |
| `basic` | Pure domain + fixture tests | Yes (`src/domain` markers) | N/A (library-shaped) |
| `leptos-wasi` | Thin Spin Leptos CQRS shell (counter-style) | Yes | `make spin …` via `ddd serve` |
| **`fullstack`** | Production SaaS (auth/org/settings, wasi-auth) | **Yes** for `aggregate`/`event`/`command` (optional `src/domain`); stubs refused | `make dev transport=both` via `ddd serve` |
| `native-api` / `worker` / `custom` | API/worker/minimal bases | Partial / stub-oriented | Make targets as generated |

The CLI is Spin-focused. Do not use `--runtime wasmtime` for CLI-generated projects unless `capabilities --json` lists it.

### Fullstack shape (locked)

`preset=fullstack` requires:

- `db=postgres`
- `transport=both`
- `ui=leptos`

Defaults already set those. Generated tree is dual-synced with `examples/fullstack-app` (no `src/domain/`; empty `[domains]` in `ddd.toml`).

## Quick scaffold: fullstack SaaS

Preferred product path:

```bash
# From crates.io after install:
ddd init my-saas --preset fullstack

# From this monorepo:
make scaffold-fullstack DIR=my-saas
# or:
cargo run -p ddd-cqrs-es-cli -- init my-saas --preset fullstack

cd my-saas
cp .env.example .env
make db-up
make dev transport=both
# http://localhost:3008
```

Init JSON includes `data.next_steps` for fullstack. `make dev` starts **Spin + wasi-auth-outbox-worker** (required for verification mail). `make spin` alone does not deliver mail.

### Fullstack command matrix

| Command | Supported? |
| --- | --- |
| `ddd check` | Yes |
| `ddd serve` / `ddd watch` | Yes → `make dev transport=…` |
| `ddd fresh` | Yes → `make db=postgres fresh` (Postgres up) |
| `ddd enable auth` / `authorization` / `passkeys` / `oauth-provider *` | Manifest only; set secrets in `.env`/Spin |
| `ddd enable grpc` / `rest` / `leptos` | Manifest bookkeeping; **no** Cargo.toml feature surgery |
| `ddd enable db *` (non-postgres) | Fails validation |
| `ddd add aggregate` | Yes — `src/domain` + `domain_app` + `domain_rest` (`/api/domain/...`) + lib/rest hooks |
| `ddd add event` / `command` | Yes — patches marker regions on the aggregate module |
| `ddd add projection\|route\|server-fn\|grpc-method\|…` | **No** — unwired product stubs refused |

Example after fullstack init:

```bash
cd my-saas
ddd add aggregate Invoice
ddd add event Invoice InvoicePaid --field amount:i64
ddd add command Invoice PayInvoice --field amount:i64
# Demo REST (process-local store):
# POST /api/domain/invoice/{id}/commands  {"CreateInvoice":{"name":"acme"}}
# GET  /api/domain/invoice/{id}
# Replace InMemoryEventStore for production; add authn/authz as needed.
```

`src/domain`, `src/domain_app`, and `src/domain_rest.rs` are **app-specific** and excluded from dual-sync.

## Project Creation (other presets)

```bash
cargo run -p ddd-cqrs-es-cli -- init my-app --preset basic --domain Invoice
cargo run -p ddd-cqrs-es-cli -- init my-app --preset leptos-wasi --domain Counter --db sqlite --runtime spin --realtime off --transport http --ui leptos
cargo run -p ddd-cqrs-es-cli -- init my-saas --preset fullstack
```

Keep Redis modes distinct:

- `--db redis` means Redis is the durable event/checkpoint/read-model store.
- `--realtime redis` means Redis is the wake/notification transport and can pair with another durable DB.

## Extending Generated Projects (basic / leptos-wasi)

Run fine-grained generators from the generated project root, or pass `--cwd <project>`:

```bash
cargo run -p ddd-cqrs-es-cli -- --cwd my-app add aggregate BillingAccount
cargo run -p ddd-cqrs-es-cli -- --cwd my-app add event Invoice InvoicePaid --field amount:i64 --field paid_at:String
cargo run -p ddd-cqrs-es-cli -- --cwd my-app add command Invoice PayInvoice --field amount:i64
cargo run -p ddd-cqrs-es-cli -- --cwd my-app add projection InvoiceLedger
cargo run -p ddd-cqrs-es-cli -- --cwd my-app add route invoice-summary --method GET --path /api/invoices/summary
```

These require `src/domain/{module}.rs` with `// ddd:…` markers. On fullstack,
prefer `add aggregate|event|command` (also wires domain_app + REST); refuse
orphan projection/route stubs.

Use `ddd enable ...` for capability wiring on non-fullstack apps:

```bash
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable db postgres
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable realtime redis
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable grpc
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable idempotency
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable snapshots
cargo run -p ddd-cqrs-es-cli -- --cwd my-app enable tracing
```

Generated projects are tracked by `ddd.toml`. If `ddd.toml` is missing, treat the app as outside the supported patching path unless the user explicitly asks to adopt it.

## File and Symbol Targeting

Do not invent unsupported target syntax. As of this skill, `ddd add event` and `ddd add command` target aggregates by manifest/domain name, not arbitrary `/path/to/file.rs:Symbol` selectors.

## Runtime Commands

```bash
cargo run -p ddd-cqrs-es-cli -- --cwd my-app --dry-run --format json serve
cargo run -p ddd-cqrs-es-cli -- --cwd my-app --dry-run --format json watch
cargo run -p ddd-cqrs-es-cli -- --cwd my-app --dry-run --format json fresh --db sqlite
```

`fresh` is reset-only. It must not be described as starting the server.

Use `ddd check` after mutations:

```bash
cargo run -p ddd-cqrs-es-cli -- --cwd my-app check
```

## Monorepo: dual-sync and drift (template editors)

When editing `examples/fullstack-app` product files that belong in the CLI template:

```bash
bash examples/fullstack-app/scripts/sync_fullstack_template.sh
bash examples/fullstack-app/scripts/sync_fullstack_template.sh check
bash scripts/regenerate-fullstack-example.sh --check   # or: make fullstack-check
```

Cargo for the template is shipped as `Cargo.toml.template` (not nested `Cargo.toml`) so `cargo package` includes the tree.

UI chrome / soft-nav / skeleton patterns: `docs/tutorial/leptos-islands-persistent-chrome.md` and `examples/fullstack-app` (not domain codegen).

## Safety Rules

- Dry-run before writing unless the user explicitly asks for immediate writes.
- Avoid `--force` until inspecting the collision and confirming overwrite is intended.
- Prefer generated marker regions and `ddd.toml` updates over ad hoc string edits (non-fullstack).
- Keep the root framework transport-agnostic; HTTP, REST, SSE, gRPC, Spin, and app wiring belong in generated apps and CLI templates.
- When changing CLI behavior, update tests and this skill if agent-facing commands change.
- On fullstack, prefer `ddd add aggregate|event|command` for product domain; refuse unwired stubs.

## Release Workflow

The library crate and CLI crate are released as a same-version pair:

```bash
make version 0.3.0-rc.6   # or next RC / stable
make publish dry-run
make publish
```

Full chain (leptos-wasi-runtime → wasi-auth → ddd → CLI):

```bash
make publish-fullstack dry-run
make publish-fullstack
make registry-check
```

Run `cargo login` first, or provide `CARGO_REGISTRY_TOKEN` in the environment.

The publish script validates matching versions and publishes `ddd_cqrs_es` before `ddd-cqrs-es-cli`.

## Verification

After editing the CLI or this skill, run the focused checks that match the change:

```bash
cargo fmt --all -- --check
cargo test -p ddd-cqrs-es-cli --all-targets
bash scripts/verify-docs.sh
bash scripts/regenerate-fullstack-example.sh --check
```

For release-flow changes, run:

```bash
bash scripts/release-crates-io.sh dry-run
```

## Roadmap

Fullstack product domain includes demo app + REST. Still manual for production:
durable event store (replace InMemory), Cedar auth on `/api/domain/*`, Leptos UI,
gRPC, and projections.
