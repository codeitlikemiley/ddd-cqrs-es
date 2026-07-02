---
title: Architecture Payoff
description: Summarize how the DDD and CQRS split keeps domain logic independent from infrastructure and runtime choices.
---

# Architecture Payoff

## 💎 The Pure DDD & CQRS Advantage

Take a moment to step back and realize what we have accomplished.

By separating **Domain Logic** (commands, aggregate invariants, and events) from **Infrastructure Concerns** (SQLite, Postgres, HTTP API protocols, network sandboxing, and runtime-specific environments), we have made our application completely robust, future-proof, and flexible.

*   Want to run your microservice as a lightweight, zero-dependency serverless edge component? **Set `DATABASE_BACKEND=sqlite`**.
*   Need to scale to enterprise workloads on AWS with thousands of events per second? **Enable `DATABASE_BACKEND=postgres`**.
*   Want to run globally distributed edge containers with serverless SQL backends? **Set `DATABASE_BACKEND=neon` or `DATABASE_BACKEND=turso`**.
*   Want MySQL for a self-hosted or managed relational backend? **Set `DATABASE_BACKEND=mysql`**.
*   Want Redis-backed event persistence and faster cross-client UI wakeups? **Set `DATABASE_BACKEND=redis` and `REALTIME_BACKEND=redis`**.
*   Want MySQL persistence with Redis wake notifications? **Set `DATABASE_BACKEND=mysql` and `REALTIME_BACKEND=redis`**.

Your domain logic does not change by a single letter. That is the outstanding power of building enterprise-grade systems with clean, decoupled **Domain-Driven Design** and **Event Sourcing**!
