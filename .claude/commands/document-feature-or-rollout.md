---
name: document-feature-or-rollout
description: Workflow command scaffold for document-feature-or-rollout in ddd-cqrs-es.
allowed_tools: ["Bash", "Read", "Write", "Grep", "Glob"]
---

# /document-feature-or-rollout

Use this workflow when working on **document-feature-or-rollout** in `ddd-cqrs-es`.

## Goal

Adds or updates documentation for a new feature, rollout plan, or production process, often in PRD or production docs.

## Common Files

- `docs/prd/*.md`
- `docs/production/*.md`
- `docs/docs.json`
- `docs/index.md`

## Suggested Sequence

1. Understand the current state and failure mode before editing.
2. Make the smallest coherent change that satisfies the workflow goal.
3. Run the most relevant verification for touched files.
4. Summarize what changed and what still needs review.

## Typical Commit Signals

- Edit or add markdown files in docs/prd/** or docs/production/**
- Update docs/docs.json or docs/index.md if needed
- Optionally update related CLI or template docs

## Notes

- Treat this as a scaffold, not a hard-coded script.
- Update the command if the workflow evolves materially.