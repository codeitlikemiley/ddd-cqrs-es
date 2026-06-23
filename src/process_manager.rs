/// Event-driven policy that emits commands in response to events.
///
/// Process managers, also called sagas, should not mutate aggregate state
/// directly. They may keep their own state and should be designed for
/// idempotent event handling.
pub trait ProcessManager<E, C> {
    /// Process manager error.
    type Error;

    /// Stable process manager name.
    fn name(&self) -> &'static str;

    /// Handles one event and returns commands to dispatch.
    fn handle(&mut self, event: &E) -> Result<Vec<C>, Self::Error>;
}
