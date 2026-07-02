---
title: Leptos WASM SSR + Spin CQRS
description: Production-grade guide for building a Leptos WASM SSR application with ddd_cqrs_es, WASI, Spin, CQRS, projections, and multiple backend options.
---

# Leptos WASM SSR + Spin CQRS: Production Implementation

In this advanced tutorial, we will design, build, and deploy a complete, production-ready, full-stack reactive application using **Leptos** (WebAssembly Server-Side Rendering), **WASI**, and **Fermyon Spin** powered by our extensible `ddd_cqrs_es` framework.

By the end of this guide, you will understand how to model a domain using Event Sourcing, implement highly optimized read-model projections, overcome the compilation limits of WebAssembly inside sandboxed microservices, and deliver a zero-latency, reactive UI using optimistic updates and server actions.

---

## 🗺️ Architectural Blueprint

Before we dive into the code, let's look at the flow of a modern, full-stack CQRS and Event Sourced system. Here is how commands flow from the interactive Leptos UI on the client, get validated and processed on the server, persist in our event store, update projections sequentially, and hydrate the reactive client-side interface:

```mermaid
sequenceDiagram
    autonumber
    actor User as 🌐 User (Browser)
    participant Client as 🖥️ Leptos Client (WASM)
    participant Server as ⚙️ Leptos Server (WASI)
    participant Domain as 🧠 Aggregate (Counter)
    participant EventDB as 🗄️ Event Store (SQLite)
    participant ReadDB as 📊 Read Model (SQLite)

    %% 1. Command Dispatch
    User->>Client: Clicks "+1" button
    Note over Client: Optimistic Update:<br/>Increment display instantly (e.g. from 5 to 6)
    Client->>Server: HTTP POST /api/increment_count (Server Function)

    %% 2. Rehydration & Handling
    Server->>EventDB: Fetch historical events for Counter ID
    EventDB-->>Server: [Incremented { amount: 1 }]
    Server->>Domain: Replay events to rebuild current state (Value = 5)
    Server->>Domain: Handle Command: Increment { amount: 1 }
    Note over Domain: Check Invariants:<br/>1. Is amount > 0?<br/>2. Will it overflow i32?
    Domain-->>Server: Ok([Incremented { amount: 1 }])

    %% 3. Persistence & Projection
    Server->>EventDB: Append new event with revision tracking
    EventDB-->>Server: Commited sequence #43
    Server->>Server: Trigger projection runner (On-the-fly Checkpoint)
    Server->>ReadDB: Update flat read model: UPDATE counter_read_model SET value = 6
    Server->>ReadDB: Save Checkpoint sequence #43

    %% 4. Response & Hydration
    Server-->>Client: HTTP 200 OK (Sync complete)
    Client->>Server: Fetch current count (or read model)
    Server-->>Client: Returns count = 6
    Note over Client: Finalizes UI state.<br/>Sync indicator glows green!
```

---

---

## Guide Sections

This implementation guide is split into focused pages so each production concern can be read and maintained independently:

1. [Domain Modeling and Pure Domain](./leptos-ssr-domain)
2. [WASM and Spin Storage](./leptos-ssr-spin-storage)
3. [Projections and Checkpointing](./leptos-ssr-projections)
4. [Leptos Server APIs](./leptos-ssr-server-api)
5. [Reactive Leptos UI](./leptos-ssr-ui)
6. [Runtime Configuration and Backends](./leptos-ssr-runtime-backends)
7. [Execution and Testing Playbook](./leptos-ssr-execution)
8. [Architecture Payoff](./leptos-ssr-architecture-payoff)

Future deployment material, such as a dedicated SpinKube production guide, should live as its own page in this runtime section instead of being appended to this overview.
