---
title: Getting Started
description: Learn how to build your first command, event, and state pipeline using infrastructure-light Event Sourcing.
---

Building an application with Event Sourcing requires a shift in how you think about application state. 

In a traditional CRUD system, the current state of an entity is stored as a single row in a database table. When a change occurs, you perform an `UPDATE` statement, overwriting the old state. This destroys the historical context of how the entity reached its current state.

In an **Event Sourced** system, the current state is never mutated directly. Instead:
1. You represent every change as an immutable historical fact called a **Domain Event**.
2. Because events represent things that have *already happened*, they are always named in the **past tense** (e.g., `AccountOpened`, `MoneyDeposited`).
3. To determine the current state of an entity, you load its entire stream of historical events from an append-only store and replay them in-memory, sequentially. This process is called **State Reconstitution** or **Replay**.

---

## Conceptual Pipeline

Here is how a transaction flows on the write path:

```
[ Client Command ] (Intent)
        │
        ▼
[ Command Handler ] (Loads Aggregate history, replays past events, recovers state)
        │
        ▼
[ Aggregate Domain Logic ] (Validates command against reconstituted state)
        │
        ├── Error: Rule Violated! (Tx Aborts)
        └── Success: Emits New Domain Events (Facts)
                │
                ▼
[ Event Store ] (Appends new events to stream using optimistic concurrency checks)
```

---

## Complete Bank Account Implementation

Let's implement a complete Bank Account aggregate. This example demonstrates how to define commands, events, domain errors, and state, and bind them together by implementing the `Aggregate` trait.

```rust
use ddd_cqrs_es::{Aggregate, DomainEvent, InMemoryEventStore, Repository, Metadata};

// =========================================================================
// 1. Define Domain Events (The Facts)
// =========================================================================
// Events must represent historical facts that have already occurred. They
// must be stable, immutable, and serializable.
#[derive(Clone, Debug, PartialEq)]
pub enum BankAccountEvent {
    AccountOpened { account_id: String, owner: String },
    MoneyDeposited { amount: u64 },
    MoneyWithdrawn { amount: u64 },
}

impl DomainEvent for BankAccountEvent {
    // Unique identifier for the event schema, useful for adapters
    fn event_type(&self) -> &'static str {
        match self {
            BankAccountEvent::AccountOpened { .. } => "bank_account_opened",
            BankAccountEvent::MoneyDeposited { .. } => "money_deposited",
            BankAccountEvent::MoneyWithdrawn { .. } => "money_withdrawn",
        }
    }
}

// =========================================================================
// 2. Define Commands (The Intent)
// =========================================================================
// Commands represent user intents or instructions. They can be rejected
// if they violate domain business rules.
pub enum BankAccountCommand {
    OpenAccount { account_id: String, owner: String },
    DepositMoney { amount: u64 },
    WithdrawMoney { amount: u64 },
}

// =========================================================================
// 3. Define Domain Errors
// =========================================================================
// Specific errors represent exactly why a business rule validation failed.
#[derive(Debug, PartialEq, Eq)]
pub enum BankAccountError {
    AccountAlreadyOpen,
    AccountNotYetOpen,
    InsufficientFunds { available: u64, requested: u64 },
    InvalidDepositAmount,
}

// =========================================================================
// 4. Define State (The Aggregate Root)
// =========================================================================
// The aggregate root maintains internal state. It is reconstituted by
// replaying events, and is used to validate incoming commands.
#[derive(Default)]
pub struct BankAccount {
    id: Option<String>,
    owner: Option<String>,
    balance: u64,
    revision: u64,
}

impl BankAccount {
    // Expose helpers for read models or assertions
    pub fn balance(&self) -> u64 {
        self.balance
    }
}

// =========================================================================
// 5. Implement the Aggregate Trait
// =========================================================================
impl Aggregate for BankAccount {
    type Id = String;
    type Command = BankAccountCommand;
    type Event = BankAccountEvent;
    type Error = BankAccountError;

    // Unique name for this type of aggregate across the store
    fn aggregate_type() -> &'static str {
        "bank_account"
    }

    // Expose the unique ID of this instance
    fn id(&self) -> Option<&Self::Id> {
        self.id.as_ref()
    }

    // Current version number of the aggregate (tracks replayed events count)
    fn revision(&self) -> u64 {
        self.revision
    }

    // Factory method to initialize an empty aggregate prior to state replay
    fn new() -> Self {
        Self::default()
    }

    // Replays past historical events to rebuild state in-memory.
    // This method MUST be completely deterministic and free of side effects.
    fn apply(&mut self, event: &Self::Event) {
        match event {
            BankAccountEvent::AccountOpened { account_id, owner } => {
                self.id = Some(account_id.clone());
                self.owner = Some(owner.clone());
                self.balance = 0;
            }
            BankAccountEvent::MoneyDeposited { amount } => {
                self.balance += amount;
            }
            BankAccountEvent::MoneyWithdrawn { amount } => {
                self.balance -= amount;
            }
        }
        self.revision += 1; // Increment aggregate stream version
    }

    // Handles incoming commands against the current replayed state.
    // Validates business invariants and returns new events or an error.
    // It must NOT mutate state directly (state is only mutated in apply()).
    fn handle(&self, command: Self::Command) -> Result<Vec<Self::Event>, Self::Error> {
        match command {
            BankAccountCommand::OpenAccount { account_id, owner } => {
                if self.id.is_some() {
                    return Err(BankAccountError::AccountAlreadyOpen);
                }
                Ok(vec![BankAccountEvent::AccountOpened { account_id, owner }])
            }
            BankAccountCommand::DepositMoney { amount } => {
                if self.id.is_none() {
                    return Err(BankAccountError::AccountNotYetOpen);
                }
                if amount == 0 {
                    return Err(BankAccountError::InvalidDepositAmount);
                }
                Ok(vec![BankAccountEvent::MoneyDeposited { amount }])
            }
            BankAccountCommand::WithdrawMoney { amount } => {
                if self.id.is_none() {
                    return Err(BankAccountError::AccountNotYetOpen);
                }
                if self.balance < amount {
                    return Err(BankAccountError::InsufficientFunds {
                        available: self.balance,
                        requested: amount,
                    });
                }
                Ok(vec![BankAccountEvent::MoneyWithdrawn { amount }])
            }
        }
    }
}
```

---

## Running a Command Execution

With our aggregate set up, we can coordinate our write path using `Repository` and an in-memory event store:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize a thread-safe, local, in-memory event store
    let store = InMemoryEventStore::<BankAccount>::new();
    
    // Bind store to the repository coordinator
    let repo = Repository::new(store);
    let account_id = "account-1".to_owned();

    println!("1. Dispatching OpenAccount command...");
    repo.execute(
        &account_id,
        BankAccountCommand::OpenAccount {
            account_id: account_id.clone(),
            owner: "Uriah".to_owned(),
        },
        Metadata::default(),
    )?;

    println!("2. Dispatching DepositMoney command...");
    repo.execute(
        &account_id,
        BankAccountCommand::DepositMoney { amount: 250 },
        Metadata::default(),
    )?;

    println!("3. Loading reconstituted state from history...");
    let loaded = repo.load(&account_id)?;
    
    // Validate replayed state is accurate
    assert_eq!(loaded.state.balance(), 250);
    assert_eq!(loaded.revision, 2);
    
    println!("Account balance is successfully reconstituted: ${}", loaded.state.balance());
    Ok(())
}
```

---

## Next Steps

Now that you have defined your aggregate, learn how the write path, read path, and policies communicate:
- Explore the [**Architecture & Design Guide**](/architecture) to understand the request cycle.
- Learn about durable databases in the [**Persistence & Storage Guide**](/persistence).
