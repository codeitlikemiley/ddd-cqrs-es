---
title: Welcome to ddd_cqrs_es
description: A lightweight, infrastructure-light Domain-Driven Design (DDD), CQRS, and Event Sourcing framework for Rust.
---

Welcome to **ddd_cqrs_es**, a lightweight Rust framework designed to help you construct highly reliable, testable, and maintainable software systems using the combined power of **Domain-Driven Design (DDD)**, **Command Query Responsibility Segregation (CQRS)**, and **Event Sourcing (ES)**.

The distinguishing design philosophy of this framework is that it is completely **infrastructure-light**. Your core domain logic—the rules that govern how your business operates—is kept entirely free of dependencies on databases, serialization formats, web frameworks, or asynchronous runtimes.

---

## Why "Infrastructure-Light" Matters

In many traditional enterprise architectures, domain logic becomes tightly coupled with database schemas, ORM libraries (like diesel or sqlx), or serialization structures. This tight coupling makes testing slow, upgrades painful, and code difficult to reason about.

By keeping the domain **pure and infrastructure-free**, we achieve three primary objectives:

1. **Uncompromised Testability:** Because the domain does not know about databases or network connections, your business rules can be tested in milliseconds using standard, in-memory unit tests.
2. **Long-Term Agility:** You can swap out your database (e.g., migrating from SQLite for local development to PostgreSQL or EventStoreDB in production) or your web framework (e.g., Axum to Actix-web) without changing a single line of domain code.
3. **Reduced Cognitive Load:** Developers writing business rules only need to focus on pure, synchronous Rust data structures and validations, without worrying about thread safety, connection pooling, or asynchronous race conditions.

---

## Core Pillars

Explore the core components of our framework:

<CardGroup cols={2}>
  <Card title="Aggregates" icon="shield-halved" href="/architecture">
    Explicit consistency boundaries that validate commands and emit historical facts (events) without mutating state directly.
  </Card>
  <Card title="Event Sourcing" icon="database" href="/persistence">
    Durable stream persistence and state reconstruction backed by robust local memory, SQLite, or PostgreSQL adapters.
  </Card>
  <Card title="Read Models (Projections)" icon="eye" href="/projections">
    Build fast, query-optimized, eventually-consistent projections that process committed events sequentially.
  </Card>
  <Card title="BDD Domain Testing" icon="vial" href="/testing">
    Assert aggregate validations cleanly using an elegant, out-of-the-box Behavior-Driven Development (BDD) test fixture.
  </Card>
</CardGroup>

---

## Detailed Conceptual Guides

To understand the core design and how to apply these patterns in your codebase, read our detailed guides:

* [**Getting Started**](/getting-started) — Learn how to write commands, events, and your first aggregate command handler.
* [**Architecture & Design**](/architecture) — Deeply understand Aggregate boundaries, the CQRS write/read split, and the lifecycle of a command.
* [**Persistence & Storage**](/persistence) — Deep dive into the append-only ledger model, database schemas, and optimistic concurrency.
* [**Projections & Read Models**](/projections) — Learn how eventually-consistent projections consume event streams and manage checkpoints.
* [**Testing & BDD**](/testing) — See why Event Sourcing is a unit-testing superpower and learn to write robust BDD tests in seconds.
