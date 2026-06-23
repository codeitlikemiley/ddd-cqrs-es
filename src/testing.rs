use crate::aggregate::Aggregate;
use std::fmt::Debug;

/// Fluent aggregate test fixture.
///
/// The fixture exercises aggregate decision logic without requiring a
/// repository or event store.
#[derive(Clone, Debug)]
pub struct AggregateFixture<A>
where
    A: Aggregate,
{
    given: Vec<A::Event>,
}

impl<A> AggregateFixture<A>
where
    A: Aggregate,
{
    /// Creates an empty fixture.
    pub fn new() -> Self {
        Self { given: Vec::new() }
    }

    /// Starts from an empty event history.
    pub fn given_no_events(mut self) -> Self {
        self.given.clear();
        self
    }

    /// Starts from a given event history.
    pub fn given(mut self, events: Vec<A::Event>) -> Self {
        self.given = events;
        self
    }

    /// Handles a command against replayed state.
    pub fn when(self, command: A::Command) -> AggregateFixtureResult<A> {
        let loaded = A::replay_events(&self.given);
        let result = loaded.state.handle(command);

        AggregateFixtureResult {
            state: loaded.state,
            revision: loaded.revision,
            result,
        }
    }
}

impl<A> Default for AggregateFixture<A>
where
    A: Aggregate,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing a command in an aggregate fixture.
#[derive(Clone, Debug)]
pub struct AggregateFixtureResult<A>
where
    A: Aggregate,
{
    state: A,
    revision: u64,
    result: Result<Vec<A::Event>, A::Error>,
}

impl<A> AggregateFixtureResult<A>
where
    A: Aggregate,
{
    /// Asserts that command handling produced exactly the expected events.
    pub fn then_expect_events(self, expected: Vec<A::Event>) -> Self
    where
        A::Event: PartialEq + Debug,
        A::Error: Debug,
    {
        assert_eq!(self.result.as_ref().unwrap(), &expected);
        self
    }

    /// Asserts that command handling produced no events.
    pub fn then_expect_no_events(self) -> Self
    where
        A::Error: Debug,
    {
        assert!(self.result.as_ref().unwrap().is_empty());
        self
    }

    /// Asserts that command handling returned the expected domain error.
    pub fn then_expect_error(self, expected: A::Error) -> Self
    where
        A::Error: PartialEq + Debug,
    {
        match &self.result {
            Ok(_) => panic!("expected aggregate error, got events"),
            Err(error) => assert_eq!(error, &expected),
        }
        self
    }

    /// Asserts against replayed aggregate state before the command.
    pub fn then_expect_state(self, assertion: impl FnOnce(&A)) -> Self {
        assertion(&self.state);
        self
    }

    /// Asserts the replayed revision before the command.
    pub fn then_expect_revision(self, expected: u64) -> Self {
        assert_eq!(self.revision, expected);
        self
    }
}
