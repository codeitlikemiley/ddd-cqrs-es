/// Dispatches commands without requiring a specific application framework.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::CommandBus;
///
/// struct MyCommandBus;
///
/// impl CommandBus<String> for MyCommandBus {
///     type Output = usize;
///     type Error = &'static str;
///
///     fn dispatch(&self, command: String) -> Result<Self::Output, Self::Error> {
///         Ok(command.len())
///     }
/// }
///
/// let bus = MyCommandBus;
/// assert_eq!(bus.dispatch("hello".to_string()), Ok(5));
/// ```
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
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::CommandHandler;
///
/// struct DepositHandler;
///
/// impl CommandHandler<u32> for DepositHandler {
///     type Output = ();
///     type Error = &'static str;
///
///     fn handle(&self, command: u32) -> Result<Self::Output, Self::Error> {
///         if command == 0 {
///             return Err("Amount must be greater than zero");
///         }
///         Ok(())
///     }
/// }
///
/// let handler = DepositHandler;
/// assert_eq!(handler.handle(100), Ok(()));
/// ```
pub trait CommandHandler<C> {
    /// Handler result.
    type Output;
    /// Handler error.
    type Error;

    /// Handles a command.
    fn handle(&self, command: C) -> Result<Self::Output, Self::Error>;
}

/// Handles a query on the read side of a CQRS application.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::QueryHandler;
///
/// struct GetBalanceQuery;
/// struct BalanceHandler;
///
/// impl QueryHandler<GetBalanceQuery> for BalanceHandler {
///     type Output = u64;
///     type Error = &'static str;
///
///     fn handle(&self, _query: GetBalanceQuery) -> Result<Self::Output, Self::Error> {
///         Ok(42)
///     }
/// }
///
/// let handler = BalanceHandler;
/// assert_eq!(handler.handle(GetBalanceQuery), Ok(42));
/// ```
pub trait QueryHandler<Q> {
    /// Query result.
    type Output;
    /// Query error.
    type Error;

    /// Executes a query.
    fn handle(&self, query: Q) -> Result<Self::Output, Self::Error>;
}
