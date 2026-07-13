---
name: add-or-update-stack-template-and-example
description: Workflow command scaffold for add-or-update-stack-template-and-example in ddd-cqrs-es.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /add-or-update-stack-template-and-example

Use this workflow when working on **add-or-update-stack-template-and-example** in `ddd-cqrs-es`.

## Goal

Adds or updates a stack (auth-stack or fullstack) both in CLI templates and in the corresponding example app, ensuring parity between template and example.

## Common Files

- `crates/ddd-cli/templates/auth-stack/**`
- `crates/ddd-cli/templates/fullstack/**`
- `examples/auth-stack/**`
- `examples/fullstack-app/**`
- `crates/ddd-cli/src/render.rs`
- `crates/ddd-cli/tests/cli.rs`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Edit or add files in crates/ddd-cli/templates/<stack-name>/** to define the template
- Edit or add files in examples/<stack-name>-app/** to provide a runnable example
- Update CLI code (crates/ddd-cli/src/render.rs, etc.) to support the template
- Update or add test coverage in crates/ddd-cli/tests/cli.rs
- Optionally update documentation (docs/cli.md) and manifest/model files

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.