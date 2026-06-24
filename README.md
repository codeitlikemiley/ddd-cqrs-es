# ddd_cqrs_es

A lightweight, infrastructure-light Domain-Driven Design (DDD), CQRS, and Event Sourcing framework for Rust.

Decouple your core business logic completely from databases, serialization, web frameworks, and asynchronous runtimes. Design pure domain aggregates, enforce transactional consistency boundaries, and build rich read models with minimal friction.

---

## Quick Installation

Add the crate as a path dependency in your `Cargo.toml`:

```toml
[dependencies]
ddd_cqrs_es = { path = "../ddd_cqrs_es" }
```

To enable durable adapters:
- **SQLite Support:** `features = ["sqlite"]`
- **PostgreSQL Support:** `features = ["postgres"]`

---

## Quick Usage

Define your command, event, and state. Execute a transaction:

```rust
use ddd_cqrs_es::{Aggregate, InMemoryEventStore, Repository, Metadata};

// 1. Define command, event, state, and implement the Aggregate trait.
// (See docs/getting-started for the complete example code)

let store = InMemoryEventStore::<BankAccount>::new();
let repo = Repository::new(store);
let account_id = "account_abc123".to_owned();

// 2. Execute business validation and persist events in a transaction
repo.execute(
    &account_id,
    BankAccountCommand::DepositMoney { amount: 100 },
    Metadata::default(),
)?;

// 3. Rebuild the aggregate state by replaying past events in-memory
let loaded = repo.load(&account_id)?;
assert_eq!(loaded.state.balance(), 100);
```

---

## Detailed Conceptual Guides

Our documentation is structured around explaining the **theoretical concepts and patterns** before jumping into code. Each guide includes extensive theoretical discussions, visual architectural diagrams, and full production-ready code.

Explore our guides in the [`/docs`](./docs) directory:

### 🚀 [Getting Started Guide](./docs/getting-started.md)
* **What you'll learn:** The core mechanics of Event Sourcing, how Aggregate state is rebuilt via history replay instead of CRUD overwrites, and how to write your first Aggregate command handler in Rust.

### 🏛️ [Architecture & Design Guide](./docs/architecture.md)
* **What you'll learn:** DDD concepts (Aggregate Roots as transactional consistency boundaries, Ubiquitous Language, Entities vs Value Objects) and the CQRS write/read responsibility split.
* Includes the comprehensive command pipeline and read model propagation sequence diagrams.

### 💾 [Persistence & Event Storage Guide](./docs/persistence.md)
* **What you'll learn:** The append-only ledger model, designing durable schemas, and why Optimistic Concurrency Control (`ExpectedRevision`) is essential for stateless horizontal scaling.
* Covers configuration of in-memory, SQLite, and PostgreSQL database adapters.

### 👁️ [Projections & Read Models Guide](./docs/projections.md)
* **What you'll learn:** Eventual consistency, asynchronous read-model materialization, sequence checkpoint tracking, and why projection appliers must be strictly idempotent.

### 🧪 [Behavior-Driven Development Guide](./docs/testing.md)
* **What you'll learn:** Why Event Sourcing is a unit-testing superpower. Write clean, fast BDD tests using the `AggregateFixture` API (`Given` history -> `When` command -> `Then` expect events).

---

## Local Documentation Server

We utilize **Mintlify** to render a beautiful, modern documentation website. To preview the documentation locally with live-reloading:

1. Install the Mint CLI:
   ```bash
   npm install -g mintlify
   ```
2. Navigate to the documentation directory and run the dev server:
   ```bash
   cd docs
   mint dev
   ```
3. To validate the configuration and check for broken links prior to shipping:
   ```bash
   mint validate
   mint broken-links
   ```
