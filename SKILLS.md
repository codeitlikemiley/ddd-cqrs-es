# Skills Index

This repository ships repository-local skills used by agent workflows.

## Available Skills

- `ddd-cli`
  - Location: `.agents/skills/ddd-cli/SKILL.md`
  - Scope: Use the `ddd` CLI for scaffolding, fine-grained generators, capability enablement, runtime command resolution, dry-run JSON, and same-version library/CLI releases.
  - Use this for:
    - creating generated apps with `ddd init`
    - extending generated apps with `ddd add ...`
    - enabling capabilities with `ddd enable ...`
    - previewing operations for agents/MCP with `--dry-run --format json`
    - release/version flows that publish `ddd_cqrs_es` and `ddd-cqrs-es-cli` together

- `leptos-wasi-cqrs`
  - Location: `.agents/skills/leptos-wasi-cqrs/SKILL.md`
  - Scope: Build, integrate, and debug full-stack CQRS/Event Sourcing applications using `ddd_cqrs_es` with Leptos WASI/Spin/Wasmtime.
  - Use this for:
    - backend/realtime matrix changes in `examples/counter-app`
    - store/runtime wiring updates
    - migration and reset behavior for backends
    - Spin trigger and SSE/realtime behavior in examples

## When to consult

- Consult `ddd-cli` before using or changing the CLI command surface, generated templates, `ddd.toml`, or CLI release workflow.
- Consult before editing counter-app backend dispatch, command contracts, or documentation that spans backends/realtime.
- Consult when publishing or validating contributor-facing workflows that depend on `make help`, `make db`, or `make realtime` behavior.

## Related source-of-truth files

- `crates/ddd-cli/src/lib.rs`
- `crates/ddd-cli/src/model.rs`
- `crates/ddd-cli/src/manifest.rs`
- `crates/ddd-cli/tests/cli.rs`
- `docs/cli.md`
- `examples/counter-app/Makefile`
- `examples/counter-app/README.md`
- `docs/docs.json`
- `docs/tutorial/leptos-ssr.md`
- `docs/production/redis.md`
