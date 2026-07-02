use std::pin::Pin;

use futures::Stream;
use spin_sdk::http::IntoResponse;
use tonic::{Request, Response, Status};

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
    let response =
        spin_sdk::http::grpc::serve(CounterServiceServer::new(CounterGrpcService), req).await;
    response.into_response()
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
        _request: Request<proto::ResetCounterRequest>,
    ) -> Result<Response<proto::CounterView>, Status> {
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
}

struct WatchState {
    last_sequence: u64,
    sent_initial: bool,
}

async fn execute(
    command: crate::domain::CounterCommand,
) -> Result<Response<proto::CounterView>, Status> {
    let view = crate::application::execute_counter_command(command)
        .await
        .map_err(|error| status_from_app_error("execute_counter_command", error))?;
    Ok(Response::new(view.into()))
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
