---
title: Domain Modeling and Pure Domain
description: Model the counter aggregate, commands, events, and pure Rust domain logic for the Leptos WASM SSR CQRS example.
---

# Domain Modeling and Pure Domain

## 1. Conceptualization & Domain Modeling

Let's model the **Counter** domain. A counter seems simple, but in an enterprise environment, every state change requires complete auditability, precise validation rules, and high scalability.

### Mapping the Domain Requirements
To model this domain, we map the requirements to core DDD and Event Sourcing patterns:

*   **Aggregate Root (`Counter`)**: The primary consistency boundary. It maintains the current counter value, tracks the stream revision (for optimistic concurrency), and ensures that all state mutations are applied sequentially.
*   **Value Object (`CounterId`)**: A type-safe newtype wrapper around a `String` representing the unique ID of our counter stream.
*   **Commands (`CounterCommand`)**: Intentions to change state. These represent the *write* operations:
    *   `Increment { amount: i32 }`: Requests to add a positive amount.
    *   `Decrement { amount: i32 }`: Requests to subtract a positive amount.
    *   `Reset`: Requests to reset the counter to zero.
*   **Events (`CounterEvent`)**: Historical, immutable facts that have occurred. These represent our *historical log*:
    *   `Incremented { amount: i32 }`
    *   `Decremented { amount: i32 }`
    *   `ResetPerformed { value: i32 }`

### Why This Matters
By separating commands (intentions) from events (facts), we separate validation from execution. 

> [!IMPORTANT]
> **Command Handling is Validative**: Commands can be rejected if they violate invariants.
> **Event Application is Infallible**: Events represent the past. Once an event is committed, it cannot be rejected or fail to apply; it must mutate the aggregate state without further checks.

---

## 2. Implementing the Pure Domain

Let's examine the full, pure domain implementation located inside `examples/counter-app/src/domain.rs`. Notice that this file has absolutely **no infrastructure dependencies** (no databases, no network frameworks). It is pure, highly testable Rust logic that implements our framework's `Aggregate` trait.

```rust
use std::fmt;
use serde::{Deserialize, Serialize};
use ddd_cqrs_es::{Aggregate, DomainEvent};

/// Type-safe newtype wrapper for the Counter Aggregate ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CounterId(pub String);

impl fmt::Display for CounterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CounterId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for CounterId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// Commands accepted by the Counter Aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterCommand {
    Increment { amount: i32 },
    Decrement { amount: i32 },
    Reset,
}

/// Domain events emitted by the Counter Aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CounterEvent {
    Incremented { amount: i32 },
    Decremented { amount: i32 },
    ResetPerformed { value: i32 },
}

impl DomainEvent for CounterEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CounterEvent::Incremented { .. } => "incremented",
            CounterEvent::Decremented { .. } => "decremented",
            CounterEvent::ResetPerformed { .. } => "reset_performed",
        }
    }
}

/// The Counter Aggregate state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    pub id: CounterId,
    pub value: i32,
    pub revision: u64,
}

impl Aggregate for Counter {
    type Id = CounterId;
    type Command = CounterCommand;
    type Event = CounterEvent;
    type Error = String;

    fn aggregate_type() -> &'static str {
        "counter"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn new() -> Self {
        Self {
            id: CounterId(String::new()),
            value: 0,
            revision: 0,
        }
    }

    /// Infallibly mutates state based on a committed event fact.
    fn apply(&mut self, event: &Self::Event) {
        match event {
            CounterEvent::Incremented { amount } => {
                self.value = self.value.saturating_add(*amount);
            }
            CounterEvent::Decremented { amount } => {
                self.value = self.value.saturating_sub(*amount);
            }
            CounterEvent::ResetPerformed { value } => {
                self.value = *value;
            }
        }
        self.revision += 1;
    }

    /// Validates a command against current state. Returns a vector of events if successful.
    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            CounterCommand::Increment { amount } => {
                if amount <= 0 {
                    return Err("amount to increment must be positive".to_string());
                }
                if self.value.checked_add(amount).is_none() {
                    return Err("increment would overflow integer boundary".to_string());
                }
                Ok(vec![CounterEvent::Incremented { amount }])
            }
            CounterCommand::Decrement { amount } => {
                if_dbg!(amount <= 0);
                if amount <= 0 {
                    return Err("amount to decrement must be positive".to_string());
                }
                if self.value.checked_sub(amount).is_none() {
                    return Err("decrement would underflow integer boundary".to_string());
                }
                Ok(vec![CounterEvent::Decremented { amount }])
            }
            CounterCommand::Reset => {
                Ok(vec![CounterEvent::ResetPerformed { value: 0 }])
            }
        }
    }

    /// Rebuilds aggregate state by replaying envelopes sequentially.
    fn replay(events: &[ddd_cqrs_es::EventEnvelope<Self::Event, Self::Id>]) -> ddd_cqrs_es::LoadedAggregate<Self> {
        let mut state = Self::new();
        let mut revision = ddd_cqrs_es::INITIAL_REVISION;

        if let Some(first) = events.first() {
            state.id = first.aggregate_id.clone();
        }

        for envelope in events {
            state.apply(&envelope.payload);
            revision = envelope.revision;
        }

        ddd_cqrs_es::LoadedAggregate { state, revision }
    }
}
```

> [!TIP]
> Notice the usage of `checked_add` and `checked_sub` in `handle`. This ensures the aggregate defends its invariants *before* accepting changes, while `apply` uses `saturating_add` as a secondary safety mechanism when applying historical facts.

---

