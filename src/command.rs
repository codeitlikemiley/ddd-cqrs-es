/// Dispatches commands without requiring a specific application framework.
pub trait CommandBus<C> {
    /// Command result returned by the bus.
    type Output;
    /// Error returned when dispatch fails.
    type Error;

    /// Dispatches a command to its handler.
    fn dispatch(&self, command: C) -> Result<Self::Output, Self::Error>;
}

/// Handles a command in application or domain code.
///
/// Aggregates use [`Aggregate::handle`](crate::Aggregate::handle) directly.
/// This trait exists for application services, command buses, and middleware.
pub trait CommandHandler<C> {
    /// Handler result.
    type Output;
    /// Handler error.
    type Error;

    /// Handles a command.
    fn handle(&self, command: C) -> Result<Self::Output, Self::Error>;
}

/// Handles a query on the read side of a CQRS application.
pub trait QueryHandler<Q> {
    /// Query result.
    type Output;
    /// Query error.
    type Error;

    /// Executes a query.
    fn handle(&self, query: Q) -> Result<Self::Output, Self::Error>;
}
