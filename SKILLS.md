# Skills Index

This repository ships repository-local skills used by agent workflows.

## Available Skills

- `ddd-cli`
  - Location: `.agents/skills/ddd-cli/SKILL.md`
  - Scope: Use the `ddd` CLI for scaffolding (including **fullstack SaaS**), fine-grained generators on basic/leptos-wasi, capability enablement, runtime command resolution, dry-run JSON, dual-sync/drift, and same-version library/CLI releases.
  - Use this for:
    - `ddd init --preset fullstack` / `make scaffold-fullstack`
    - creating domain apps with `ddd init --preset basic|leptos-wasi`
    - extending non-fullstack apps with `ddd add ...` (refused on fullstack)
    - enabling capabilities with `ddd enable ...`
    - previewing operations for agents/MCP with `--dry-run --format json`
    - release/version flows (`make publish`, `make publish-fullstack`)

- `leptos-wasi-cqrs`
  - Location: `.agents/skills/leptos-wasi-cqrs/SKILL.md`
  - Scope: Build, integrate, and debug **counter-app** CQRS/Event Sourcing with Leptos WASI/Spin/Wasmtime.
  - Use this for:
    - backend/realtime matrix changes in `examples/counter-app`
    - store/runtime wiring updates
    - migration and reset behavior for backends
    - Spin trigger and SSE/realtime behavior in examples
  - For fullstack product UI (soft-nav chrome, skeletons): see
    `docs/tutorial/leptos-islands-persistent-chrome.md` and `ddd-cli` skill.

## When to consult

- Consult `ddd-cli` before using or changing the CLI command surface, generated templates (including fullstack), `ddd.toml`, or CLI release workflow.
- Consult `leptos-wasi-cqrs` before editing counter-app backend dispatch, command contracts, or multi-backend docs.
- Consult when publishing or validating contributor-facing workflows that depend on `make help`, `make db`, or `make realtime` behavior.

## Related source-of-truth files

- `crates/ddd-cli/src/lib.rs`
- `crates/ddd-cli/src/model.rs`
- `crates/ddd-cli/src/manifest.rs`
- `crates/ddd-cli/tests/cli.rs`
- `docs/cli.md`
- `examples/counter-app/Makefile`
- `examples/counter-app/README.md`
- `examples/fullstack-app/README.md`
- `examples/fullstack-app/scripts/sync_fullstack_template.sh`
- `docs/docs.json`
- `docs/tutorial/leptos-ssr.md`
- `docs/tutorial/leptos-islands-persistent-chrome.md`
- `docs/production/wasi-auth-fullstack.md`
- `docs/production/redis.md`
