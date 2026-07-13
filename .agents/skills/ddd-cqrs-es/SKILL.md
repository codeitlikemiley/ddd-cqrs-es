```markdown
# ddd-cqrs-es Development Patterns

> Auto-generated skill from repository analysis

## Overview

This skill teaches you how to contribute effectively to the `ddd-cqrs-es` Rust codebase, which implements Domain-Driven Design (DDD), Command Query Responsibility Segregation (CQRS), and Event Sourcing (ES) patterns. You'll learn the project's coding conventions, commit patterns, and the main workflows for adding or updating stack templates, documenting features, hardening implementations, and preparing releases. The guide also covers how to structure code, write tests, and use repository-specific commands to streamline your work.

## Coding Conventions

**File Naming:**  
- Use camelCase for file names.
  - Example: `eventStore.rs`, `userService.rs`

**Import Style:**  
- Use relative imports.
  - Example:
    ```rust
    mod eventStore;
    use super::eventStore::EventStore;
    ```

**Export Style:**  
- Use named exports.
  - Example:
    ```rust
    pub struct EventStore { /* ... */ }
    pub fn new_event_store() -> EventStore { /* ... */ }
    ```

**Commit Patterns:**  
- Prefixes: `feat`, `docs`, `release`, `refactor`, `fix`
- Example commit messages:
  - `feat: add user authentication flow`
  - `fix: correct event serialization bug`
  - `docs: update CLI usage in readme`

## Workflows

### Add or Update Stack Template and Example
**Trigger:** When introducing a new stack template or updating an existing stack's implementation and tests  
**Command:** `/sync-stack-template`

1. Edit or add files in `crates/ddd-cli/templates/<stack-name>/**` to define the template.
2. Edit or add files in `examples/<stack-name>-app/**` to provide a runnable example.
3. Update CLI code (e.g., `crates/ddd-cli/src/render.rs`) to support the template.
4. Update or add test coverage in `crates/ddd-cli/tests/cli.rs`.
5. Optionally update documentation (`docs/cli.md`) and manifest/model files.

**Example:**
```bash
# Add a new template file
touch crates/ddd-cli/templates/fullstack/src/newFeature.rs

# Update the example app
cp crates/ddd-cli/templates/fullstack/src/newFeature.rs examples/fullstack-app/src/newFeature.rs
```

---

### Document Feature or Rollout
**Trigger:** When documenting a new feature, rollout, or updating the implementation tracker  
**Command:** `/document-rollout`

1. Edit or add markdown files in `docs/prd/**` or `docs/production/**`.
2. Update `docs/docs.json` or `docs/index.md` if needed.
3. Optionally update related CLI or template docs.

**Example:**
```bash
# Add a new PRD document
nano docs/prd/new-feature.md

# Update documentation index
nano docs/index.md
```

---

### Harden or Enhance Stack Implementation
**Trigger:** When enhancing, fixing, or hardening a stack's implementation (e.g., authentication flows, production gates)  
**Command:** `/harden-stack`

1. Edit implementation files in `examples/<stack-name>-app/src/**`.
2. Edit corresponding files in `crates/ddd-cli/templates/<stack-name>/src/**`.
3. Update scripts, configuration, or proto files as needed in both template and example.
4. Update CLI tests if necessary.

**Example:**
```bash
# Harden authentication logic
vim examples/auth-stack-app/src/auth.rs
vim crates/ddd-cli/templates/auth-stack/src/auth.rs

# Sync scripts
cp examples/auth-stack/scripts/setup.sh crates/ddd-cli/templates/auth-stack/scripts/setup.sh
```

---

### Release Preparation
**Trigger:** When preparing for a new release or release candidate  
**Command:** `/prepare-release`

1. Update version in `Cargo.toml` and `crates/ddd-cli/Cargo.toml`.
2. Update `CHANGELOG.md`.
3. Update `README.md` and `docs/index.md` as needed.
4. Update example app manifests or docs.

**Example:**
```bash
# Bump version
sed -i 's/version = "0.1.0"/version = "0.2.0"/' Cargo.toml crates/ddd-cli/Cargo.toml

# Update changelog
nano CHANGELOG.md
```

## Testing Patterns

- **Framework:** Unknown (Rust native or custom)
- **Test File Pattern:** `*.test.ts` (Note: This pattern suggests some TypeScript-based tests, possibly for CLI or cross-language validation.)
- **Typical Test Location:**  
  - Rust tests: `crates/ddd-cli/tests/cli.rs`
  - TypeScript tests: files matching `*.test.ts`

**Example Rust Test:**
```rust
#[test]
fn test_cli_template_generation() {
    // Test logic here
}
```

**Example TypeScript Test:**
```typescript
// example.test.ts
import { runCli } from './cli';

test('CLI generates template', () => {
  expect(runCli('fullstack')).toBeTruthy();
});
```

## Commands

| Command                | Purpose                                                              |
|------------------------|----------------------------------------------------------------------|
| /sync-stack-template   | Add or update a stack template and its example app                   |
| /document-rollout      | Add or update documentation for a feature, rollout, or production    |
| /harden-stack          | Harden, fix, or enhance a stack implementation and keep in sync      |
| /prepare-release       | Prepare the repository for a new release                             |
```