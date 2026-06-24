/// Event-driven policy that emits commands in response to events.
///
/// Process managers, also called sagas, should not mutate aggregate state
/// directly. They may keep their own state and should be designed for
/// idempotent event handling.
///
/// # Example
///
/// ```rust
/// use ddd_cqrs_es::ProcessManager;
///
/// #[derive(Clone)]
/// enum OrderEvent {
///     Placed { order_id: String },
/// }
///
/// #[derive(Clone, Debug, PartialEq)]
/// enum ShippingCommand {
///     ShipOrder { order_id: String },
/// }
///
/// struct ShippingSaga;
///
/// impl ProcessManager<OrderEvent, ShippingCommand> for ShippingSaga {
///     type Error = std::convert::Infallible;
///
///     fn name(&self) -> &'static str { "shipping_saga" }
///
///     fn handle(&mut self, event: &OrderEvent) -> Result<Vec<ShippingCommand>, Self::Error> {
///         match event {
///             OrderEvent::Placed { order_id } => Ok(vec![
///                 ShippingCommand::ShipOrder { order_id: order_id.clone() }
///             ]),
///         }
///     }
/// }
///
/// let mut saga = ShippingSaga;
/// let commands = saga.handle(&OrderEvent::Placed { order_id: "order-123".to_string() }).unwrap();
/// assert_eq!(commands, vec![ShippingCommand::ShipOrder { order_id: "order-123".to_string() }]);
/// ```
pub trait ProcessManager<E, C> {
    /// Process manager error.
    type Error;

    /// Stable process manager name.
    fn name(&self) -> &'static str;

    /// Handles one event and returns commands to dispatch.
    fn handle(&mut self, event: &E) -> Result<Vec<C>, Self::Error>;
}
