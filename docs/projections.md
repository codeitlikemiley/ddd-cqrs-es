# Projections

Projections build read models from committed event envelopes. They should be
idempotent because runners may retry after a failure.

Implement `Projection<E, Id>`:

```rust
use ddd_cqrs_es::{EventEnvelope, Projection};
use std::collections::HashMap;

enum CounterEvent {
    Created,
    Incremented { by: u64 },
}

#[derive(Default)]
struct CounterSummary {
    values: HashMap<String, u64>,
}

impl Projection<CounterEvent, String> for CounterSummary {
    type Error = ();

    fn name(&self) -> &'static str {
        "counter_summary"
    }

    fn apply(&mut self, event: &EventEnvelope<CounterEvent, String>) -> Result<(), Self::Error> {
        let value = self.values.entry(event.aggregate_id.clone()).or_default();
        match event.payload {
            CounterEvent::Created => {}
            CounterEvent::Incremented { by } => *value += by,
        }
        Ok(())
    }
}
```

Use `InMemoryProjectionRunner` for local replay:

```rust
let mut runner = InMemoryProjectionRunner::new(CounterSummary::default());
runner.run::<Counter, _>(&store)?;
```

The runner advances its checkpoint only after `Projection::apply` succeeds.
Persistent projection runners should store checkpoints durably.
