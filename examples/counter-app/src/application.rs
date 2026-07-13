use crate::app::CounterViewDto;
use crate::domain::{Counter, CounterCommand, CounterId};
use crate::error::{CounterAppError, CounterAppResult};
use crate::store::MultiBackendEventStore;
use ddd_cqrs_es::AsyncRepository;

pub async fn get_counter_view() -> CounterAppResult<CounterViewDto> {
    crate::store::get_counter_view_db().await
}

pub async fn execute_counter_command(command: CounterCommand) -> CounterAppResult<CounterViewDto> {
    let event_store = MultiBackendEventStore::<Counter>::new();
    let repository = AsyncRepository::new(event_store);
    let aggregate_id = CounterId("global".to_string());
    let command_name = counter_command_name(&command);
    tracing::info!(
        command = command_name,
        aggregate_id = %aggregate_id,
        backend = %crate::store::get_backend(),
        realtime = %crate::store::get_realtime_backend(),
        "starting counter command"
    );

    const COMMAND_CONCURRENCY_RETRIES: usize = 6;
    let mut attempts = 0;
    let (loaded, committed_events) = loop {
        match repository
            .execute_returning_state(
                &aggregate_id,
                command.clone(),
                ddd_cqrs_es::Metadata::default(),
            )
            .await
        {
            Ok(outcome) => break outcome,
            Err(error)
                if attempts < COMMAND_CONCURRENCY_RETRIES
                    && is_retryable_counter_write_conflict(&error) =>
            {
                attempts += 1;
                tracing::warn!(
                    attempts,
                    error = %error,
                    "retrying counter command after write conflict"
                );
                wasip3::clocks::monotonic_clock::wait_for(attempts as u64 * 5_000_000).await;
            }
            Err(error) => return Err(CounterAppError::from_repository_error(error)),
        }
    };

    let mut view = get_counter_view().await?;
    view.count = loaded.state.value;
    if let Some(last_sequence) = committed_events.last().and_then(|event| event.sequence) {
        view.last_sequence = last_sequence;
    }
    if let Err(error) = crate::store::publish_counter_realtime(&view).await {
        log_nonfatal_error("counter realtime publish failed", &error);
    }
    if let Err(error) = crate::store::catch_up_counter_projection().await {
        log_nonfatal_error("counter projection catch-up failed", &error);
    }

    tracing::info!(
        count = view.count,
        last_sequence = view.last_sequence,
        "counter command completed"
    );

    Ok(view)
}

pub async fn execute_counter_command_authorized(
    command: CounterCommand,
    auth: &crate::auth::CounterAuthContext,
) -> CounterAppResult<CounterViewDto> {
    let permission = match &command {
        CounterCommand::Reset => "counter.reset",
        CounterCommand::Increment { .. } | CounterCommand::Decrement { .. } => "counter.change",
    };
    auth.authorize(permission).await?;
    execute_counter_command(command).await
}

fn counter_command_name(command: &CounterCommand) -> &'static str {
    match command {
        CounterCommand::Increment { .. } => "increment",
        CounterCommand::Decrement { .. } => "decrement",
        CounterCommand::Reset => "reset",
    }
}

fn log_nonfatal_error(message: &'static str, error: &CounterAppError) {
    tracing::error!(
        error = %error,
        error_code = error.public_code(),
        public_message = %error.public_message(),
        "{message}"
    );
}

fn is_retryable_counter_write_conflict(
    error: &ddd_cqrs_es::RepositoryError<String, ddd_cqrs_es::EventStoreError>,
) -> bool {
    match error {
        ddd_cqrs_es::RepositoryError::Concurrency(_) => true,
        ddd_cqrs_es::RepositoryError::Store(ddd_cqrs_es::EventStoreError::Backend(message)) => {
            let message = message.to_ascii_lowercase();
            (message.contains("unique")
                || message.contains("duplicate")
                || message.contains("constraint"))
                && (message.contains("revision")
                    || message.contains("aggregate")
                    || message.contains("idx_aggregate_revision"))
        }
        _ => false,
    }
}
