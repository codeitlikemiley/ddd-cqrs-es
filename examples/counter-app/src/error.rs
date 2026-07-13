use ddd_cqrs_es::{ConcurrencyError, EventStoreError, RepositoryError};
use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

pub type CounterAppResult<T> = Result<T, CounterAppError>;

#[derive(Debug, Error)]
pub enum CounterAppError {
    #[error("authentication is required")]
    AuthRequired,
    #[error("counter operation is forbidden")]
    Forbidden,
    #[error("validation error: {message}")]
    Validation { message: String },
    #[error("domain error: {message}")]
    Domain { message: String },
    #[error("concurrency error: {source}")]
    Concurrency {
        #[source]
        source: ConcurrencyError,
    },
    #[error("event store error: {source}")]
    Store {
        #[source]
        source: EventStoreError,
    },
    #[error("event store error: {message}")]
    StoreMessage { message: String },
    #[error("read model error: {message}")]
    ReadModel { message: String },
    #[error("projection error: {message}")]
    Projection { message: String },
    #[error("realtime error: {message}")]
    Realtime { message: String },
    #[error("serialization error: {message}")]
    Serialization { message: String },
    #[error("configuration error: {message}")]
    Configuration { message: String },
    #[error("transport error: {message}")]
    Transport { message: String },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CounterErrorResponse {
    pub error: CounterErrorPayload,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CounterErrorPayload {
    pub code: &'static str,
    pub message: String,
}

impl CounterAppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub fn store_message(message: impl Into<String>) -> Self {
        Self::StoreMessage {
            message: message.into(),
        }
    }

    pub fn read_model(message: impl Into<String>) -> Self {
        Self::ReadModel {
            message: message.into(),
        }
    }

    pub fn projection(message: impl Into<String>) -> Self {
        Self::Projection {
            message: message.into(),
        }
    }

    pub fn realtime(message: impl Into<String>) -> Self {
        Self::Realtime {
            message: message.into(),
        }
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            message: message.into(),
        }
    }

    pub fn from_event_store_error(source: EventStoreError) -> Self {
        Self::Store { source }
    }

    pub fn from_repository_error(
        error: RepositoryError<String, EventStoreError>,
    ) -> CounterAppError {
        match error {
            RepositoryError::Domain(message) => CounterAppError::Domain { message },
            RepositoryError::Concurrency(source) => CounterAppError::Concurrency { source },
            RepositoryError::Store(source) => CounterAppError::Store { source },
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::AuthRequired => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Validation { .. } | Self::Domain { .. } => StatusCode::BAD_REQUEST,
            Self::Concurrency { .. } => StatusCode::CONFLICT,
            Self::Configuration { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Store { source } if is_unavailable_store_error(source) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::Store { .. }
            | Self::StoreMessage { .. }
            | Self::ReadModel { .. }
            | Self::Projection { .. }
            | Self::Realtime { .. }
            | Self::Serialization { .. }
            | Self::Transport { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn public_code(&self) -> &'static str {
        match self {
            Self::AuthRequired => "authentication_required",
            Self::Forbidden => "forbidden",
            Self::Validation { .. } => "validation",
            Self::Domain { .. } => "domain",
            Self::Concurrency { .. } => "concurrency",
            Self::Store { .. } | Self::StoreMessage { .. } => "store",
            Self::ReadModel { .. } => "read_model",
            Self::Projection { .. } => "projection",
            Self::Realtime { .. } => "realtime",
            Self::Serialization { .. } => "serialization",
            Self::Configuration { .. } => "configuration",
            Self::Transport { .. } => "transport",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::AuthRequired => "authentication is required".to_string(),
            Self::Forbidden => "counter operation is forbidden".to_string(),
            Self::Validation { message } | Self::Domain { message } => message.clone(),
            Self::Concurrency { .. } => "counter command conflicted; retry the request".to_string(),
            Self::Configuration { .. } => {
                "counter service is not configured for this operation".to_string()
            }
            Self::Store { source } if is_unavailable_store_error(source) => {
                "counter storage is unavailable".to_string()
            }
            Self::Store { .. }
            | Self::StoreMessage { .. }
            | Self::ReadModel { .. }
            | Self::Projection { .. }
            | Self::Realtime { .. }
            | Self::Serialization { .. }
            | Self::Transport { .. } => "counter service failed; check server logs".to_string(),
        }
    }

    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::AuthRequired
                | Self::Forbidden
                | Self::Validation { .. }
                | Self::Domain { .. }
                | Self::Concurrency { .. }
        )
    }

    pub fn response_body(&self) -> CounterErrorResponse {
        CounterErrorResponse {
            error: CounterErrorPayload {
                code: self.public_code(),
                message: self.public_message(),
            },
        }
    }

    pub fn server_fn_error(&self) -> server_fn::ServerFnError {
        server_fn::ServerFnError::new(self.public_message())
    }

    #[cfg(feature = "spin-grpc")]
    pub fn grpc_status(&self) -> tonic::Status {
        tonic::Status::new(self.grpc_code(), self.public_message())
    }

    #[cfg(feature = "spin-grpc")]
    pub fn grpc_code(&self) -> tonic::Code {
        match self {
            Self::AuthRequired => tonic::Code::Unauthenticated,
            Self::Forbidden => tonic::Code::PermissionDenied,
            Self::Validation { .. } | Self::Domain { .. } => tonic::Code::InvalidArgument,
            Self::Concurrency { .. } => tonic::Code::Aborted,
            Self::Configuration { .. } => tonic::Code::Unavailable,
            Self::Store { source } if is_unavailable_store_error(source) => {
                tonic::Code::Unavailable
            }
            Self::Store { .. }
            | Self::StoreMessage { .. }
            | Self::ReadModel { .. }
            | Self::Projection { .. }
            | Self::Realtime { .. }
            | Self::Serialization { .. }
            | Self::Transport { .. } => tonic::Code::Internal,
        }
    }
}

fn is_unavailable_store_error(error: &EventStoreError) -> bool {
    matches!(
        error,
        EventStoreError::Connection(_) | EventStoreError::ConnectionWithSource { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddd_cqrs_es::{ExpectedRevision, RepositoryError};

    #[test]
    fn repository_domain_error_maps_to_domain_error() {
        let error = CounterAppError::from_repository_error(RepositoryError::Domain(
            "amount to increment must be positive".to_string(),
        ));

        assert!(matches!(error, CounterAppError::Domain { .. }));
    }

    #[test]
    fn repository_concurrency_error_maps_to_http_conflict() {
        let error = CounterAppError::from_repository_error(RepositoryError::Concurrency(
            ConcurrencyError::WrongExpectedRevision {
                expected: ExpectedRevision::Exact(2),
                actual: 1,
            },
        ));

        assert_eq!(error.http_status(), StatusCode::CONFLICT);
    }

    #[test]
    fn connection_store_error_maps_to_service_unavailable() {
        let error = CounterAppError::from_event_store_error(EventStoreError::Connection(
            "database is offline".to_string(),
        ));

        assert_eq!(error.http_status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn internal_store_error_hides_backend_message_from_public_response() {
        let error = CounterAppError::from_event_store_error(EventStoreError::Backend(
            "raw adapter failure".to_string(),
        ));

        assert_eq!(
            error.response_body().error.message,
            "counter service failed; check server logs"
        );
    }

    #[test]
    fn server_function_error_uses_public_message() {
        let error = CounterAppError::from_event_store_error(EventStoreError::Backend(
            "raw adapter failure".to_string(),
        ));

        assert_eq!(
            error.server_fn_error(),
            server_fn::ServerFnError::new("counter service failed; check server logs")
        );
    }

    #[cfg(feature = "spin-grpc")]
    #[test]
    fn validation_error_maps_to_grpc_invalid_argument() {
        let error = CounterAppError::validation("amount must be positive");

        assert_eq!(error.grpc_code(), tonic::Code::InvalidArgument);
    }

    #[cfg(feature = "spin-grpc")]
    #[test]
    fn concurrency_error_maps_to_grpc_aborted() {
        let error = CounterAppError::Concurrency {
            source: ConcurrencyError::StreamAlreadyExists,
        };

        assert_eq!(error.grpc_code(), tonic::Code::Aborted);
    }
}
