---
title: ddd CLI
description: Scaffold, extend, validate, and run ddd_cqrs_es applications from a Laravel-style CLI.
---

The `ddd` CLI is the fastest way to start a `ddd_cqrs_es` application and keep generated projects consistent as they grow.

It is published by the `ddd-cli` crate, installs a binary named `ddd`, and is released with the same version as `ddd_cqrs_es`.

Use it for:

- new app scaffolding with opinionated presets
- fine-grained additions like events, commands, routes, projections, and tests
- capability wiring for Redis, gRPC, tracing, snapshots, and idempotency
- Spin runtime command resolution
- agent/MCP workflows that need deterministic dry-run JSON before writing files

## Install

Install the published CLI:

```bash
cargo install ddd-cli --locked
```

Check that it is available:

```bash
ddd --help
ddd capabilities --json
```

When developing this repository locally, use the workspace binary instead:

```bash
cargo run -p ddd-cli -- <args>
```

For example:

```bash
cargo run -p ddd-cli -- init billing --preset basic --domain Invoice
```

## Quick Start

Create a small domain-only project:

```bash
ddd init billing --preset basic --domain Invoice
cd billing
ddd check
```

Add a domain event:

```bash
ddd add event Invoice InvoicePaid --field amount:i64 --field paid_at:String
```

Add a command:

```bash
ddd add command Invoice PayInvoice --field amount:i64
```

Preview any write before applying it:

```bash
ddd --dry-run --format json add event Invoice PaymentFailed --field reason:String
```

The dry-run output reports the planned file operations without changing files.

## Command Model

Every command supports the same global controls:

| Option | Purpose |
| --- | --- |
| `--cwd <path>` | Run against a generated project without changing the shell directory. |
| `--dry-run` | Preview file operations or runtime command resolution without writing or executing. |
| `--format text\|json` | Choose human text or machine-readable JSON output. |
| `--force` | Allow overwriting files that would otherwise be protected. Inspect collisions first. |
| `--yes` | Reserved for non-interactive confirmations as workflows grow. |

The main command groups are:

| Command | Use |
| --- | --- |
| `ddd init <path>` | Create a generated project and `ddd.toml` manifest. |
| `ddd add ...` | Add aggregates, events, commands, routes, tests, projections, and related stubs. |
| `ddd enable ...` | Wire capabilities into an existing generated project. |
| `ddd serve` | Resolve and run the app through the selected Spin runtime command. |
| `ddd watch` | Resolve a rebuild/restart loop for the selected runtime command. |
| `ddd fresh` | Reset schema/data only. This does not start the server. |
| `ddd doctor` | Inspect required local tools. |
| `ddd check` | Validate the generated project manifest and required files. |
| `ddd matrix` | Print supported backend/realtime/transport combinations. |
| `ddd capabilities --json` | Expose the machine-readable CLI contract for agents and MCP tools. |

## Presets

Choose a preset with `ddd init --preset <preset>`.

| Preset | Best For | Default Shape |
| --- | --- | --- |
| `basic` | Learning the framework or building a pure domain crate. | Aggregate, command, event, fixture test, and in-memory example. |
| `leptos-wasi` | Full-stack app scaffolding for Leptos WASI on Spin. | Domain/application/store/server boundaries with REST/SSE and optional gRPC. |
| `native-api` | Native Rust API service shape. | Axum-style API scaffold with native SQL adapter features. |
| `worker` | Projection or process-manager workers. | Worker entrypoint plus projection/process-manager-oriented stubs. |
| `custom` | A minimal base for explicit, agent-chosen capabilities. | Starts from the basic shape and lets you add capabilities intentionally. |

Examples:

```bash
ddd init billing --preset basic --domain Invoice
ddd init counter-app --preset leptos-wasi --domain Counter --db sqlite --runtime spin --transport http --ui leptos
ddd init counter-grpc --preset leptos-wasi --domain Counter --db postgres --runtime spin --transport both --ui leptos
ddd init projector --preset worker --domain Invoice --db mysql --realtime polling
```

Current CLI-generated apps are Spin-focused. The runtime value is `spin`.

## Generated Manifest

Every generated project includes `ddd.toml`. The CLI uses this file to know what it can safely patch later.

Example shape:

```toml
[project]
name = "billing"
preset = "basic"
runtime = "spin"
db = "sqlite"
realtime = "off"
transport = "http"
ui = "none"

[capabilities]
enabled = []

[domains.invoice]
aggregate = "Invoice"
module = "invoice"
commands = ["CreateInvoice"]
events = ["InvoiceCreated"]
```

If a project does not have `ddd.toml`, treat it as outside the supported generated-project patching path unless you intentionally adopt it.

## Add Domain Code

Run `ddd add ...` from the generated project root, or pass `--cwd <project>`.

Add a second aggregate:

```bash
ddd add aggregate BillingAccount
```

Add an event to an existing aggregate:

```bash
ddd add event Invoice InvoicePaid --field amount:i64 --field paid_at:String --event-type invoice_paid
```

Add a command:

```bash
ddd add command Invoice PayInvoice --field amount:i64
```

Field syntax is `name:RustType`. The CLI inserts generated variants into marker regions in the generated domain module and updates `ddd.toml`.

Available `add` targets:

```text
aggregate
event
command
error
projection
query
process-manager
snapshot
upcaster
route
grpc-method
server-fn
rest-endpoint
test
```

Common examples:

```bash
ddd add projection InvoiceLedger
ddd add query InvoiceSummary
ddd add process-manager PaymentSaga
ddd add snapshot InvoiceSnapshot
ddd add upcaster InvoicePaid --from 1 --to 2
ddd add route invoice-summary --method GET --path /api/invoices/summary
ddd add rest-endpoint invoice-payments --method POST --path /api/invoices/payments
ddd add server-fn pay-invoice
ddd add grpc-method pay-invoice
ddd add test invoice-payment
```

## Enable Capabilities

Use `ddd enable ...` when the project exists and you want to wire a capability into `ddd.toml` and, where applicable, `Cargo.toml` feature flags.

```bash
ddd enable db postgres
ddd enable db mysql
ddd enable redis-store
ddd enable realtime redis
ddd enable grpc
ddd enable rest
ddd enable leptos
ddd enable idempotency
ddd enable snapshots
ddd enable tracing
```

Use dry-run JSON before enabling a capability in automation:

```bash
ddd --dry-run --format json enable realtime redis
```

## Runtime Matrix

The CLI currently scaffolds Spin-focused apps.

Supported values:

| Axis | Values |
| --- | --- |
| Runtime | `spin` |
| DB | `sqlite`, `postgres`, `neon`, `supabase`, `turso`, `mysql`, `redis` |
| Realtime | `off`, `polling`, `redis` |
| Transport | `http`, `grpc`, `both` |
| UI | `none`, `leptos` |

Use the CLI to inspect the live matrix:

```bash
ddd matrix
ddd capabilities --json
```

Redis has two separate meanings:

- `db=redis` means Redis is the durable event/checkpoint/read-model store.
- `realtime=redis` means Redis is only the wake/notification transport unless `db=redis` is also selected.

Spin supports `transport=http`, `transport=grpc`, and `transport=both`.

## Serve, Watch, and Fresh

The runtime commands read from `ddd.toml` and can be overridden with flags.

Preview the command:

```bash
ddd --dry-run --format json serve
```

Serve the app:

```bash
ddd serve
ddd serve --db postgres --realtime redis --transport http
```

Watch and restart:

```bash
ddd watch
```

Reset data only:

```bash
ddd fresh --db sqlite
```

`fresh` is reset-only. It should not start the server.

## Agent and MCP Workflow

Agents should use JSON dry-runs before changing files:

```bash
ddd --cwd billing --dry-run --format json add event Invoice InvoicePaid --field amount:i64
```

The report includes:

```json
{
  "status": "planned",
  "message": "project extension complete",
  "operations": [
    {
      "action": "update",
      "path": "src/domain/invoice.rs",
      "bytes": 2048,
      "description": "add domain event"
    }
  ]
}
```

A safe agent loop is:

1. Run `ddd capabilities --json`.
2. Run `ddd matrix` if runtime/backend choices matter.
3. Run the mutating command with `--dry-run --format json`.
4. Inspect `operations`.
5. Apply the same command without `--dry-run`.
6. Run `ddd check`.
7. Run project tests.

## File and Symbol Targeting

The current CLI targets generated projects through `ddd.toml`, aggregate names, and marker regions.

This syntax is not currently implemented:

```text
/path/to/file.rs:Struct
/path/to/file.rs:EnumVariant
/path/to/file.rs:function_name
```

If you need path or symbol targeting, add explicit CLI support and tests first, or patch the file manually with Rust-aware edits. Do not pass unsupported selector syntax to `ddd`.

## Release Pairing

The library and CLI are versioned together.

For maintainers:

```bash
make version 0.2.5
make publish dry-run
```

Reliable publish dry-run forms are:

```bash
make publish dry-run
make publish -- --dry-run
```

Real publish:

```bash
CARGO_REGISTRY_TOKEN=<token> make publish
```

The release script validates that `ddd_cqrs_es` and `ddd-cli` have matching versions, then publishes `ddd_cqrs_es` before `ddd-cli`.

## Troubleshooting

`ddd check` fails with missing generated files:

- Run it from the generated project root or pass `--cwd <project>`.
- Confirm `ddd.toml`, `Cargo.toml`, and `src/domain/mod.rs` exist.

`ddd add event` cannot find an aggregate:

- Use the aggregate name from `ddd.toml`.
- Run `ddd add aggregate <Name>` first if the aggregate does not exist.

`--runtime wasmtime` is rejected:

- The CLI-generated runtime is currently Spin-only.
- Use `spin`, or check `ddd capabilities --json` after upgrading the CLI.

A file already exists:

- Inspect the file before using `--force`.
- Prefer dry-run JSON to see exactly which path is colliding.
