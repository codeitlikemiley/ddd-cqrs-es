use ddd_cqrs_es::{Aggregate, DomainEvent};
use serde::{Deserialize, Serialize};
use std::fmt;

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
                if amount <= 0 {
                    return Err("amount to decrement must be positive".to_string());
                }
                if self.value.checked_sub(amount).is_none() {
                    return Err("decrement would underflow integer boundary".to_string());
                }
                Ok(vec![CounterEvent::Decremented { amount }])
            }
            CounterCommand::Reset => Ok(vec![CounterEvent::ResetPerformed { value: 0 }]),
        }
    }

    fn replay(
        events: &[ddd_cqrs_es::EventEnvelope<Self::Event, Self::Id>],
    ) -> ddd_cqrs_es::LoadedAggregate<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_id_display_and_serialization() {
        let id = CounterId("my-counter".to_string());
        assert_eq!(id.to_string(), "my-counter");

        let serialized = serde_json::to_string(&id).unwrap();
        assert_eq!(serialized, "\"my-counter\"");

        let deserialized: CounterId = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, id);
    }

    #[test]
    fn test_initial_state() {
        let counter = Counter::new();
        assert_eq!(counter.value, 0);
        assert_eq!(counter.revision, 0);
        assert_eq!(counter.id, CounterId(String::new()));
    }

    #[test]
    fn test_increment_command() {
        let counter = Counter::new();
        let events = counter
            .handle(CounterCommand::Increment { amount: 5 })
            .unwrap();
        assert_eq!(events, vec![CounterEvent::Incremented { amount: 5 }]);
    }

    #[test]
    fn test_increment_validation_errors() {
        let counter = Counter::new();

        // Negative amount
        let err = counter
            .handle(CounterCommand::Increment { amount: -5 })
            .unwrap_err();
        assert_eq!(err, "amount to increment must be positive");

        // Zero amount
        let err = counter
            .handle(CounterCommand::Increment { amount: 0 })
            .unwrap_err();
        assert_eq!(err, "amount to increment must be positive");

        // Overflow
        let mut max_counter = Counter::new();
        max_counter.value = i32::MAX;
        let err = max_counter
            .handle(CounterCommand::Increment { amount: 1 })
            .unwrap_err();
        assert_eq!(err, "increment would overflow integer boundary");
    }

    #[test]
    fn test_decrement_command() {
        let counter = Counter::new();
        let events = counter
            .handle(CounterCommand::Decrement { amount: 5 })
            .unwrap();
        assert_eq!(events, vec![CounterEvent::Decremented { amount: 5 }]);
    }

    #[test]
    fn test_decrement_validation_errors() {
        let counter = Counter::new();

        // Negative amount
        let err = counter
            .handle(CounterCommand::Decrement { amount: -10 })
            .unwrap_err();
        assert_eq!(err, "amount to decrement must be positive");

        // Zero amount
        let err = counter
            .handle(CounterCommand::Decrement { amount: 0 })
            .unwrap_err();
        assert_eq!(err, "amount to decrement must be positive");

        // Underflow
        let mut min_counter = Counter::new();
        min_counter.value = i32::MIN;
        let err = min_counter
            .handle(CounterCommand::Decrement { amount: 1 })
            .unwrap_err();
        assert_eq!(err, "decrement would underflow integer boundary");
    }

    #[test]
    fn test_reset_command() {
        let mut counter = Counter::new();
        counter.value = 42;
        let events = counter.handle(CounterCommand::Reset).unwrap();
        assert_eq!(events, vec![CounterEvent::ResetPerformed { value: 0 }]);
    }

    #[test]
    fn test_apply_events() {
        let mut counter = Counter::new();

        counter.apply(&CounterEvent::Incremented { amount: 10 });
        assert_eq!(counter.value, 10);
        assert_eq!(counter.revision, 1);

        counter.apply(&CounterEvent::Decremented { amount: 3 });
        assert_eq!(counter.value, 7);
        assert_eq!(counter.revision, 2);

        counter.apply(&CounterEvent::ResetPerformed { value: 0 });
        assert_eq!(counter.value, 0);
        assert_eq!(counter.revision, 3);
    }

    #[test]
    fn test_replay_raw_events_from_zero() {
        let events = vec![
            CounterEvent::Incremented { amount: 15 },
            CounterEvent::Decremented { amount: 5 },
            CounterEvent::Incremented { amount: 2 },
        ];
        let loaded = Counter::replay_raw_events_from_zero(&events);
        assert_eq!(loaded.state.value, 12);
        assert_eq!(loaded.revision, 3);
    }
}
