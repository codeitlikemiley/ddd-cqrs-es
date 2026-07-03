---
name: ddd-cli
description: Use when scaffolding, extending, validating, serving, watching, or publishing ddd_cqrs_es applications through the `ddd` CLI; especially for agent/MCP workflows that need dry-run JSON, `ddd.toml` manifests, fine-grained `ddd add ...` commands, capability enablement, Spin runtime presets, or same-version library/CLI release handling.
---

# ddd CLI Skill

## Core Rule

Prefer the `ddd` CLI over hand-written scaffolding when the target is a generated `ddd_cqrs_es` app or a new app created from this repository's templates.

In this repository, run the development binary as:

```bash
rtk cargo run -p ddd-cli -- <args>
```

After publishing or installing from crates.io, the user-facing command is:

```bash
ddd <args>
```

Follow local repo instructions for command wrappers. In this checkout, shell commands must be prefixed with `rtk`.

## First Moves

Before generating or mutating code:

1. Discover the live command surface:
   ```bash
   rtk cargo run -p ddd-cli -- capabilities --json
   ```
2. For runtime/backend combinations, inspect:
   ```bash
   rtk cargo run -p ddd-cli -- matrix
   ```
3. For any mutating command, preview first with dry-run JSON:
   ```bash
   rtk cargo run -p ddd-cli -- --dry-run --format json <command>
   ```
4. Inspect the planned `operations` before running the same command without `--dry-run`.

The JSON report is the agent/MCP contract. Expect fields like `status`, `message`, `operations`, `command`, and `data`.

## Project Creation

Create new projects with `ddd init <path>`. Prefer explicit options when an agent is producing reproducible work:

```bash
rtk cargo run -p ddd-cli -- init my-app --preset basic --domain Invoice
rtk cargo run -p ddd-cli -- init my-app --preset leptos-wasi --domain Counter --db sqlite --runtime spin --realtime off --transport http --ui leptos
```

Supported presets are discovered from `capabilities --json`; current presets are `basic`, `leptos-wasi`, `native-api`, `worker`, and `custom`.

The CLI is Spin-focused. Do not use `--runtime wasmtime` for CLI-generated projects unless the CLI implementation adds it again and `capabilities --json` confirms it.

Keep Redis modes distinct:

- `--db redis` means Redis is the durable event/checkpoint/read-model store.
- `--realtime redis` means Redis is the wake/notification transport and can pair with another durable DB.

## Extending Generated Projects

Run fine-grained generators from the generated project root, or pass `--cwd <project>`:

```bash
rtk cargo run -p ddd-cli -- --cwd my-app add aggregate BillingAccount
rtk cargo run -p ddd-cli -- --cwd my-app add event Invoice InvoicePaid --field amount:i64 --field paid_at:String
rtk cargo run -p ddd-cli -- --cwd my-app add command Invoice PayInvoice --field amount:i64
rtk cargo run -p ddd-cli -- --cwd my-app add projection InvoiceLedger
rtk cargo run -p ddd-cli -- --cwd my-app add route invoice-summary --method GET --path /api/invoices/summary
```

Use `ddd enable ...` for capability wiring:

```bash
rtk cargo run -p ddd-cli -- --cwd my-app enable db postgres
rtk cargo run -p ddd-cli -- --cwd my-app enable realtime redis
rtk cargo run -p ddd-cli -- --cwd my-app enable grpc
rtk cargo run -p ddd-cli -- --cwd my-app enable idempotency
rtk cargo run -p ddd-cli -- --cwd my-app enable snapshots
rtk cargo run -p ddd-cli -- --cwd my-app enable tracing
```

Generated projects are tracked by `ddd.toml`. If `ddd.toml` is missing, treat the app as outside the supported patching path unless the user explicitly asks to adopt it.

## File and Symbol Targeting

Do not invent unsupported target syntax. As of this skill, `ddd add event` and `ddd add command` target aggregates by manifest/domain name, not arbitrary `/path/to/file.rs:Symbol` selectors.

If a user asks for path or symbol targeting such as `/path/to/file.rs:Struct`, first inspect the current CLI parser and tests. If the syntax is not implemented, either add CLI support with tests or patch the file manually using structured Rust-aware edits; do not pass imaginary flags to `ddd`.

## Runtime Commands

Resolve runtime commands through the CLI so agents can preview them:

```bash
rtk cargo run -p ddd-cli -- --cwd my-app --dry-run --format json serve
rtk cargo run -p ddd-cli -- --cwd my-app --dry-run --format json watch
rtk cargo run -p ddd-cli -- --cwd my-app --dry-run --format json fresh --db sqlite
```

`fresh` is reset-only. It must not be described as starting the server.

Use `ddd check` after mutations:

```bash
rtk cargo run -p ddd-cli -- --cwd my-app check
```

## Safety Rules

- Dry-run before writing unless the user explicitly asks for immediate writes.
- Avoid `--force` until inspecting the collision and confirming overwrite is intended.
- Prefer generated marker regions and `ddd.toml` updates over ad hoc string edits.
- Keep the root framework transport-agnostic; HTTP, REST, SSE, gRPC, Spin, and app wiring belong in generated apps and CLI templates.
- When changing CLI behavior, update tests and this skill if agent-facing commands change.

## Release Workflow

The library crate and CLI crate are released as a same-version pair:

```bash
rtk make version 0.2.5
rtk make publish dry-run
```

`make publish dry-run` and `make publish -- --dry-run` are the reliable dry-run forms. Real publish uses:

```bash
CARGO_REGISTRY_TOKEN=<token> rtk make publish
```

The publish script validates matching versions and publishes `ddd_cqrs_es` before `ddd-cli`.

## Verification

After editing the CLI or this skill, run the focused checks that match the change:

```bash
rtk python3 /Users/uriah/.codex/skills/.system/skill-creator/scripts/quick_validate.py .agents/skills/ddd-cli
rtk cargo fmt --all -- --check
rtk cargo test -p ddd-cli --all-targets
```

If the validator fails because `PyYAML` is missing, create a temporary venv, install `PyYAML` there, and rerun the same validator from that venv. Do not add validator dependencies to this repository unless the user asks for repo-managed skill tooling.

For release-flow changes, run:

```bash
rtk bash scripts/release-crates-io.sh dry-run
```
