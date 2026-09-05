use crate::command::{CommandBus, IdempotentCommandBus};
use crate::event::EventEnvelope;
use crate::idempotency::IdempotencyKey;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[cfg(feature = "async")]
use crate::async_api::{AsyncCommandBus, AsyncIdempotentCommandBus};

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

    /// Handles one committed envelope and returns commands to dispatch.
    ///
    /// The default implementation forwards to [`Self::handle`] with the
    /// envelope payload. Override when the saga needs stable
    /// [`EventEnvelope::event_id`] or global [`EventEnvelope::sequence`].
    fn handle_envelope<Id>(
        &mut self,
        envelope: &EventEnvelope<E, Id>,
    ) -> Result<Vec<C>, Self::Error> {
        self.handle(&envelope.payload)
    }
}

/// Builds the idempotency key for one command emitted by a process manager.
///
/// Keys are stable across checkpoint resumes and event re-deliveries:
/// `{manager_name}:{event_id}:{command_index}`.
pub fn process_manager_command_idempotency_key(
    manager_name: &str,
    event_id: &str,
    index: usize,
) -> IdempotencyKey {
    IdempotencyKey::new(format!("{manager_name}:{event_id}:{index}"))
}

/// Error returned by process-manager runners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessManagerRunnerError<ProcessError, CommandError> {
    /// Process manager event handling failed.
    ProcessManager(ProcessError),
    /// Command dispatch failed.
    CommandBus(CommandError),
    /// Checkpoint persistence failed after one or more commands were dispatched.
    Checkpoint {
        /// Index of the command whose checkpoint could not be persisted.
        index: usize,
        /// Checkpoint store error message.
        message: String,
    },
}

impl<ProcessError, CommandError> Display for ProcessManagerRunnerError<ProcessError, CommandError>
where
    ProcessError: Display,
    CommandError: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessManagerRunnerError::ProcessManager(error) => Display::fmt(error, f),
            ProcessManagerRunnerError::CommandBus(error) => Display::fmt(error, f),
            ProcessManagerRunnerError::Checkpoint { index, message } => {
                write!(f, "checkpoint save failed after command {index}: {message}")
            }
        }
    }
}

impl<ProcessError, CommandError> std::error::Error
    for ProcessManagerRunnerError<ProcessError, CommandError>
where
    ProcessError: std::error::Error + 'static,
    CommandError: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProcessManagerRunnerError::ProcessManager(error) => Some(error),
            ProcessManagerRunnerError::CommandBus(error) => Some(error),
            ProcessManagerRunnerError::Checkpoint { .. } => None,
        }
    }
}

/// Runs a process manager and dispatches emitted commands through a command bus.
#[derive(Clone, Debug)]
pub struct ProcessManagerRunner<P, B> {
    process_manager: P,
    command_bus: B,
}

impl<P, B> ProcessManagerRunner<P, B> {
    /// Creates a process-manager runner.
    pub fn new(process_manager: P, command_bus: B) -> Self {
        Self {
            process_manager,
            command_bus,
        }
    }

    /// Returns the wrapped process manager.
    pub fn process_manager(&self) -> &P {
        &self.process_manager
    }

    /// Returns the wrapped process manager mutably.
    pub fn process_manager_mut(&mut self) -> &mut P {
        &mut self.process_manager
    }

    /// Returns the command bus.
    pub fn command_bus(&self) -> &B {
        &self.command_bus
    }

    /// Returns the command bus mutably.
    pub fn command_bus_mut(&mut self) -> &mut B {
        &mut self.command_bus
    }

    /// Consumes the runner and returns the wrapped process manager and command bus.
    pub fn into_parts(self) -> (P, B) {
        (self.process_manager, self.command_bus)
    }
}

impl<P, B> ProcessManagerRunner<P, B> {
    /// Handles one event and dispatches all commands emitted by the process manager.
    #[expect(
        clippy::type_complexity,
        reason = "runner result type names both process-manager and command-bus errors"
    )]
    pub fn run<E, C>(
        &mut self,
        event: &E,
    ) -> Result<Vec<B::Output>, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        B: CommandBus<C>,
    {
        let commands = self
            .process_manager
            .handle(event)
            .map_err(ProcessManagerRunnerError::ProcessManager)?;
        let mut outputs = Vec::with_capacity(commands.len());

        for command in commands {
            outputs.push(
                self.command_bus
                    .dispatch(command)
                    .map_err(ProcessManagerRunnerError::CommandBus)?,
            );
        }

        Ok(outputs)
    }
}

/// Async runner that dispatches emitted commands through an async command bus.
#[cfg(feature = "async")]
#[derive(Clone, Debug)]
pub struct AsyncProcessManagerRunner<P, B> {
    process_manager: P,
    command_bus: B,
}

#[cfg(feature = "async")]
impl<P, B> AsyncProcessManagerRunner<P, B> {
    /// Creates an async process-manager runner.
    pub fn new(process_manager: P, command_bus: B) -> Self {
        Self {
            process_manager,
            command_bus,
        }
    }

    /// Returns the wrapped process manager.
    pub fn process_manager(&self) -> &P {
        &self.process_manager
    }

    /// Returns the wrapped process manager mutably.
    pub fn process_manager_mut(&mut self) -> &mut P {
        &mut self.process_manager
    }

    /// Returns the command bus.
    pub fn command_bus(&self) -> &B {
        &self.command_bus
    }

    /// Returns the command bus mutably.
    pub fn command_bus_mut(&mut self) -> &mut B {
        &mut self.command_bus
    }

    /// Consumes the runner and returns the wrapped process manager and command bus.
    pub fn into_parts(self) -> (P, B) {
        (self.process_manager, self.command_bus)
    }
}

#[cfg(feature = "async")]
impl<P, B> AsyncProcessManagerRunner<P, B> {
    /// Handles one event and dispatches all commands emitted by the process manager.
    pub async fn run<E, C>(
        &mut self,
        event: &E,
    ) -> Result<Vec<B::Output>, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        B: AsyncCommandBus<C>,
    {
        let commands = self
            .process_manager
            .handle(event)
            .map_err(ProcessManagerRunnerError::ProcessManager)?;
        let mut outputs = Vec::with_capacity(commands.len());

        for command in commands {
            outputs.push(
                self.command_bus
                    .dispatch(command)
                    .await
                    .map_err(ProcessManagerRunnerError::CommandBus)?,
            );
        }

        Ok(outputs)
    }
}

/// Checkpoint for partially dispatched process-manager command batches.
pub trait ProcessManagerDispatchCheckpoint: Send + Sync {
    fn load_dispatch_index(
        &self,
        manager_name: &str,
        event_id: &str,
    ) -> Result<usize, Box<dyn Error + Send + Sync>>;

    fn save_dispatch_index(
        &self,
        manager_name: &str,
        event_id: &str,
        index: usize,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// Clears the checkpoint after every command in a batch succeeds.
    fn clear_dispatch_index(
        &self,
        manager_name: &str,
        event_id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let _ = (manager_name, event_id);
        Ok(())
    }
}

impl ProcessManagerDispatchCheckpoint for () {
    fn load_dispatch_index(
        &self,
        _manager_name: &str,
        _event_id: &str,
    ) -> Result<usize, Box<dyn Error + Send + Sync>> {
        Ok(0)
    }

    fn save_dispatch_index(
        &self,
        _manager_name: &str,
        _event_id: &str,
        _index: usize,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

/// Outcome of a checkpointed process-manager run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessManagerRunResult<O, E> {
    pub dispatched: Vec<O>,
    pub failed_index: Option<usize>,
    pub error: Option<E>,
}

impl<P, B> ProcessManagerRunner<P, B> {
    pub fn run_envelope_with_checkpoint<E, C, Id, CP>(
        &mut self,
        envelope: &EventEnvelope<E, Id>,
        checkpoint: &CP,
    ) -> ProcessManagerRunResult<B::Output, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        B: IdempotentCommandBus<C>,
        CP: ProcessManagerDispatchCheckpoint,
        Id: AsRef<str>,
    {
        let event_id = envelope.event_id.as_str();
        let manager_name = self.process_manager.name();
        let start_index = checkpoint
            .load_dispatch_index(manager_name, event_id)
            .unwrap_or(0);

        let commands = match self.process_manager.handle_envelope(envelope) {
            Ok(commands) => commands,
            Err(error) => {
                return ProcessManagerRunResult {
                    dispatched: Vec::new(),
                    failed_index: None,
                    error: Some(ProcessManagerRunnerError::ProcessManager(error)),
                };
            }
        };

        let mut dispatched = Vec::with_capacity(commands.len().saturating_sub(start_index));
        for (index, command) in commands.into_iter().enumerate().skip(start_index) {
            let idempotency_key =
                process_manager_command_idempotency_key(manager_name, event_id, index);
            match self
                .command_bus
                .dispatch_idempotent(idempotency_key, command)
            {
                Ok(output) => {
                    dispatched.push(output);
                    if let Err(error) =
                        checkpoint.save_dispatch_index(manager_name, event_id, index + 1)
                    {
                        return ProcessManagerRunResult {
                            dispatched,
                            failed_index: None,
                            error: Some(ProcessManagerRunnerError::Checkpoint {
                                index,
                                message: error.to_string(),
                            }),
                        };
                    }
                }
                Err(error) => {
                    return ProcessManagerRunResult {
                        dispatched,
                        failed_index: Some(index),
                        error: Some(ProcessManagerRunnerError::CommandBus(error)),
                    };
                }
            }
        }

        if let Err(error) = checkpoint.clear_dispatch_index(manager_name, event_id) {
            return ProcessManagerRunResult {
                dispatched,
                failed_index: None,
                error: Some(ProcessManagerRunnerError::Checkpoint {
                    index: start_index.saturating_sub(1),
                    message: error.to_string(),
                }),
            };
        }

        ProcessManagerRunResult {
            dispatched,
            failed_index: None,
            error: None,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn run_envelope_strict<E, C, Id, CP>(
        &mut self,
        envelope: &EventEnvelope<E, Id>,
        checkpoint: &CP,
    ) -> Result<Vec<B::Output>, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        B: IdempotentCommandBus<C>,
        CP: ProcessManagerDispatchCheckpoint,
        Id: AsRef<str>,
    {
        let result = self.run_envelope_with_checkpoint(envelope, checkpoint);
        match result.error {
            None => Ok(result.dispatched),
            Some(error) => Err(error),
        }
    }
}

#[cfg(feature = "async")]
impl<P, B> AsyncProcessManagerRunner<P, B> {
    pub async fn run_envelope_with_checkpoint<E, C, Id, CP>(
        &mut self,
        envelope: &EventEnvelope<E, Id>,
        checkpoint: &CP,
    ) -> ProcessManagerRunResult<B::Output, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        C: Send + Sync + 'static,
        B: AsyncCommandBus<C> + AsyncIdempotentCommandBus<C>,
        CP: ProcessManagerDispatchCheckpoint,
        Id: AsRef<str>,
    {
        let event_id = envelope.event_id.as_str();
        let manager_name = self.process_manager.name();
        let start_index = checkpoint
            .load_dispatch_index(manager_name, event_id)
            .unwrap_or(0);

        let commands = match self.process_manager.handle_envelope(envelope) {
            Ok(commands) => commands,
            Err(error) => {
                return ProcessManagerRunResult {
                    dispatched: Vec::new(),
                    failed_index: None,
                    error: Some(ProcessManagerRunnerError::ProcessManager(error)),
                };
            }
        };

        let mut dispatched = Vec::with_capacity(commands.len().saturating_sub(start_index));
        for (index, command) in commands.into_iter().enumerate().skip(start_index) {
            let idempotency_key =
                process_manager_command_idempotency_key(manager_name, event_id, index);
            match self
                .command_bus
                .dispatch_idempotent(idempotency_key, command)
                .await
            {
                Ok(output) => {
                    dispatched.push(output);
                    if let Err(error) =
                        checkpoint.save_dispatch_index(manager_name, event_id, index + 1)
                    {
                        return ProcessManagerRunResult {
                            dispatched,
                            failed_index: None,
                            error: Some(ProcessManagerRunnerError::Checkpoint {
                                index,
                                message: error.to_string(),
                            }),
                        };
                    }
                }
                Err(error) => {
                    return ProcessManagerRunResult {
                        dispatched,
                        failed_index: Some(index),
                        error: Some(ProcessManagerRunnerError::CommandBus(error)),
                    };
                }
            }
        }

        if let Err(error) = checkpoint.clear_dispatch_index(manager_name, event_id) {
            return ProcessManagerRunResult {
                dispatched,
                failed_index: None,
                error: Some(ProcessManagerRunnerError::Checkpoint {
                    index: start_index.saturating_sub(1),
                    message: error.to_string(),
                }),
            };
        }

        ProcessManagerRunResult {
            dispatched,
            failed_index: None,
            error: None,
        }
    }

    pub async fn run_envelope_strict<E, C, Id, CP>(
        &mut self,
        envelope: &EventEnvelope<E, Id>,
        checkpoint: &CP,
    ) -> Result<Vec<B::Output>, ProcessManagerRunnerError<P::Error, B::Error>>
    where
        P: ProcessManager<E, C>,
        C: Send + Sync + 'static,
        B: AsyncCommandBus<C> + AsyncIdempotentCommandBus<C>,
        CP: ProcessManagerDispatchCheckpoint,
        Id: AsRef<str>,
    {
        let result = self
            .run_envelope_with_checkpoint(envelope, checkpoint)
            .await;
        match result.error {
            None => Ok(result.dispatched),
            Some(error) => Err(error),
        }
    }
}
