use std::pin::Pin;

use futures::Stream;
use tonic::{Request, Response, Status};

const MAX_GRPC_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_INBOUND_STREAM_MESSAGES: usize = 100;
const MAX_INBOUND_STREAM_NANOS: u64 = 5 * 60 * 1_000_000_000;

pub mod proto {
    tonic::include_proto!("counter.v1");
}

use proto::counter_service_server::{CounterService, CounterServiceServer};

struct CounterGrpcService;

pub fn is_grpc_request(req: &spin_sdk::http::Request) -> bool {
    if req.uri().path().starts_with("/counter.v1.CounterService/") {
        return true;
    }

    req.headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/grpc"))
}

pub async fn serve(
    req: spin_sdk::http::Request,
) -> Result<wasip3::http::types::Response, wasip3::http::types::ErrorCode> {
    let may_idle_before_first_frame = matches!(
        req.uri().path(),
        "/counter.v1.CounterService/WatchCounter" | "/counter.v1.CounterService/Interact"
    );
    let service = CounterServiceServer::new(CounterGrpcService)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
        .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES);
    let response = spin_sdk::http::grpc::serve(service, req).await;
    let response = if may_idle_before_first_frame {
        wasi_auth::spin_grpc::normalize_trailers_only_response(response)
    } else {
        wasi_auth::spin_grpc::normalize_trailers_only_response_awaiting_first_frame(response).await
    };
    wasi_auth::spin_grpc::into_final_wasi_response(response)
}

#[tonic::async_trait]
impl CounterService for CounterGrpcService {
    async fn get_counter_view(
        &self,
        _request: Request<proto::GetCounterViewRequest>,
    ) -> Result<Response<proto::CounterView>, Status> {
        let view = crate::application::get_counter_view()
            .await
            .map_err(|error| status_from_app_error("get_counter_view", error))?;
        Ok(Response::new(view.into()))
    }

    async fn increment(
        &self,
        request: Request<proto::ChangeCounterRequest>,
    ) -> Result<Response<proto::CounterView>, Status> {
        authorize_counter_request(
            &crate::auth::CounterAuthContext::from_grpc_metadata(request.metadata()),
            "counter.change",
        )
        .await?;
        let amount = request.into_inner().amount;
        if amount <= 0 {
            return Err(status_from_app_error(
                "increment.validation",
                crate::error::CounterAppError::validation("amount must be positive"),
            ));
        }

        execute(crate::domain::CounterCommand::Increment { amount }).await
    }

    async fn decrement(
        &self,
        request: Request<proto::ChangeCounterRequest>,
    ) -> Result<Response<proto::CounterView>, Status> {
        authorize_counter_request(
            &crate::auth::CounterAuthContext::from_grpc_metadata(request.metadata()),
            "counter.change",
        )
        .await?;
        let amount = request.into_inner().amount;
        if amount <= 0 {
            return Err(status_from_app_error(
                "decrement.validation",
                crate::error::CounterAppError::validation("amount must be positive"),
            ));
        }

        execute(crate::domain::CounterCommand::Decrement { amount }).await
    }

    async fn reset(
        &self,
        request: Request<proto::ResetCounterRequest>,
    ) -> Result<Response<proto::CounterView>, Status> {
        authorize_counter_request(
            &crate::auth::CounterAuthContext::from_grpc_metadata(request.metadata()),
            "counter.reset",
        )
        .await?;
        execute(crate::domain::CounterCommand::Reset).await
    }

    type WatchCounterStream =
        Pin<Box<dyn Stream<Item = Result<proto::CounterRealtimeMessage, Status>> + Send>>;

    async fn watch_counter(
        &self,
        request: Request<proto::WatchCounterRequest>,
    ) -> Result<Response<Self::WatchCounterStream>, Status> {
        let state = WatchState {
            last_sequence: request.into_inner().last_sequence,
            sent_initial: false,
        };

        let stream = futures::stream::unfold(state, |mut state| async move {
            if !state.sent_initial {
                state.sent_initial = true;
                match crate::application::get_counter_view().await {
                    Ok(view) => {
                        state.last_sequence = view.last_sequence;
                        let message = proto::CounterRealtimeMessage {
                            last_sequence: view.last_sequence,
                            view: Some(view.into()),
                        };
                        return Some((Ok(message), state));
                    }
                    Err(error) => {
                        return Some((
                            Err(status_from_app_error("watch_counter.initial", error)),
                            state,
                        ));
                    }
                }
            }

            loop {
                match crate::store::counter_realtime_message_after(state.last_sequence).await {
                    Ok(Some(message)) => {
                        state.last_sequence = message.last_sequence;
                        return Some((Ok(message.into()), state));
                    }
                    Ok(None) => {
                        wasip3::clocks::monotonic_clock::wait_for(100_000_000).await;
                    }
                    Err(error) => {
                        wasip3::clocks::monotonic_clock::wait_for(1_000_000_000).await;
                        return Some((
                            Err(status_from_app_error("watch_counter.poll", error)),
                            state,
                        ));
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn apply_changes(
        &self,
        request: Request<tonic::Streaming<proto::ChangeCounterRequest>>,
    ) -> Result<Response<proto::CounterView>, Status> {
        let auth = crate::auth::CounterAuthContext::from_grpc_metadata(request.metadata());
        let started_at = wasip3::clocks::monotonic_clock::now();
        let mut inbound = request.into_inner();
        let mut count = 0_usize;
        let mut latest = None;

        while let Some(change) = inbound.message().await? {
            count += 1;
            enforce_inbound_stream_bounds(count, started_at)?;
            let command = command_from_change(change)?;
            authorize_counter_request(&auth, permission_for_command(&command)).await?;
            latest = Some(execute_view(command).await?);
        }

        let latest = latest.ok_or_else(|| Status::invalid_argument("change stream is empty"))?;
        Ok(Response::new(latest.into()))
    }

    type InteractStream =
        Pin<Box<dyn Stream<Item = Result<proto::CounterRealtimeMessage, Status>> + Send>>;

    async fn interact(
        &self,
        request: Request<tonic::Streaming<proto::CounterClientMessage>>,
    ) -> Result<Response<Self::InteractStream>, Status> {
        let auth = crate::auth::CounterAuthContext::from_grpc_metadata(request.metadata());
        let state = InteractState {
            inbound: request.into_inner(),
            auth,
            started_at: wasip3::clocks::monotonic_clock::now(),
            received: 0,
            terminated: false,
        };
        let stream = futures::stream::unfold(state, |mut state| async move {
            if state.terminated {
                return None;
            }

            let result = interact_next(&mut state).await;
            if result.is_err() {
                state.terminated = true;
            }
            Some((result, state))
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

struct WatchState {
    last_sequence: u64,
    sent_initial: bool,
}

struct InteractState {
    inbound: tonic::Streaming<proto::CounterClientMessage>,
    auth: crate::auth::CounterAuthContext,
    started_at: u64,
    received: usize,
    terminated: bool,
}

async fn interact_next(state: &mut InteractState) -> Result<proto::CounterRealtimeMessage, Status> {
    let message = state
        .inbound
        .message()
        .await?
        .ok_or_else(|| Status::cancelled("client closed the interaction stream"))?;
    state.received += 1;
    enforce_inbound_stream_bounds(state.received, state.started_at)?;

    let view = match message.message {
        Some(proto::counter_client_message::Message::Watch(watch)) => {
            match crate::store::counter_realtime_message_after(watch.last_sequence)
                .await
                .map_err(|error| status_from_app_error("interact.watch", error))?
            {
                Some(message) => message.view,
                None => crate::application::get_counter_view()
                    .await
                    .map_err(|error| status_from_app_error("interact.watch.current", error))?,
            }
        }
        Some(proto::counter_client_message::Message::Change(change)) => {
            let command = command_from_change(change)?;
            authorize_counter_request(&state.auth, permission_for_command(&command)).await?;
            execute_view(command).await?
        }
        None => return Err(Status::invalid_argument("interaction message is empty")),
    };
    Ok(proto::CounterRealtimeMessage {
        last_sequence: view.last_sequence,
        view: Some(view.into()),
    })
}

fn command_from_change(
    change: proto::ChangeCounterRequest,
) -> Result<crate::domain::CounterCommand, Status> {
    match proto::ChangeOperation::try_from(change.operation)
        .unwrap_or(proto::ChangeOperation::Unspecified)
    {
        proto::ChangeOperation::Increment if change.amount > 0 => {
            Ok(crate::domain::CounterCommand::Increment {
                amount: change.amount,
            })
        }
        proto::ChangeOperation::Decrement if change.amount > 0 => {
            Ok(crate::domain::CounterCommand::Decrement {
                amount: change.amount,
            })
        }
        proto::ChangeOperation::Reset => Ok(crate::domain::CounterCommand::Reset),
        proto::ChangeOperation::Increment | proto::ChangeOperation::Decrement => {
            Err(Status::invalid_argument("change amount must be positive"))
        }
        proto::ChangeOperation::Unspecified => {
            Err(Status::invalid_argument("change operation is required"))
        }
    }
}

fn permission_for_command(command: &crate::domain::CounterCommand) -> &'static str {
    match command {
        crate::domain::CounterCommand::Reset => "counter.reset",
        crate::domain::CounterCommand::Increment { .. }
        | crate::domain::CounterCommand::Decrement { .. } => "counter.change",
    }
}

fn enforce_inbound_stream_bounds(count: usize, started_at: u64) -> Result<(), Status> {
    if count > MAX_INBOUND_STREAM_MESSAGES {
        return Err(Status::resource_exhausted(
            "stream exceeds the 100-message limit",
        ));
    }
    if wasip3::clocks::monotonic_clock::now().saturating_sub(started_at) > MAX_INBOUND_STREAM_NANOS
    {
        return Err(Status::deadline_exceeded(
            "stream exceeds the five-minute limit",
        ));
    }
    Ok(())
}

async fn execute(
    command: crate::domain::CounterCommand,
) -> Result<Response<proto::CounterView>, Status> {
    let view = crate::application::execute_counter_command(command)
        .await
        .map_err(|error| status_from_app_error("execute_counter_command", error))?;
    Ok(Response::new(view.into()))
}

async fn execute_view(
    command: crate::domain::CounterCommand,
) -> Result<crate::app::CounterViewDto, Status> {
    crate::application::execute_counter_command(command)
        .await
        .map_err(|error| status_from_app_error("execute_counter_command", error))
}

async fn authorize_counter_request(
    auth: &crate::auth::CounterAuthContext,
    permission: &str,
) -> Result<(), Status> {
    auth.authorize(permission)
        .await
        .map_err(|error| status_from_app_error("authorize_counter_request", error))
}

fn status_from_app_error(operation: &'static str, error: crate::error::CounterAppError) -> Status {
    if error.is_client_error() {
        tracing::warn!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "counter gRPC request rejected"
        );
    } else {
        tracing::error!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "counter gRPC request failed"
        );
    }
    error.grpc_status()
}

impl From<crate::app::EventLogDto> for proto::EventLog {
    fn from(value: crate::app::EventLogDto) -> Self {
        Self {
            sequence: value.sequence,
            event_type: value.event_type,
            revision: value.revision,
            payload: value.payload,
            recorded_at: value.recorded_at,
        }
    }
}

impl From<crate::app::CounterViewDto> for proto::CounterView {
    fn from(value: crate::app::CounterViewDto) -> Self {
        Self {
            count: value.count,
            latest_events: value.latest_events.into_iter().map(Into::into).collect(),
            last_sequence: value.last_sequence,
            realtime_enabled: value.realtime_enabled,
        }
    }
}

impl From<crate::app::CounterRealtimeMessage> for proto::CounterRealtimeMessage {
    fn from(value: crate::app::CounterRealtimeMessage) -> Self {
        Self {
            view: Some(value.view.into()),
            last_sequence: value.last_sequence,
        }
    }
}
