use http::StatusCode;
use serde::Serialize;
use thiserror::Error;

pub type AuthStackResult<T> = Result<T, AuthStackError>;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AuthStackError {
    #[error("validation error: {message}")]
    Validation { message: String },
    #[error("authentication is required")]
    AuthRequired,
    #[error("credentials are invalid")]
    InvalidCredentials,
    #[error("token is invalid")]
    InvalidToken,
    #[error("session expired")]
    SessionExpired,
    #[error("permission denied")]
    Forbidden,
    #[error("not found: {message}")]
    NotFound { message: String },
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("request rate limit exceeded; retry after {retry_after_seconds} seconds")]
    RateLimited { retry_after_seconds: u64 },
    #[error("configuration error: {message}")]
    Configuration { message: String },
    #[error("store error: {message}")]
    Store { message: String },
    #[error("serialization error: {message}")]
    Serialization { message: String },
    #[error("transport error: {message}")]
    Transport { message: String },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AuthErrorResponse {
    pub error: AuthErrorPayload,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AuthErrorPayload {
    pub code: &'static str,
    pub message: String,
}

impl AuthStackError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub fn store(message: impl Into<String>) -> Self {
        Self::Store {
            message: message.into(),
        }
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
        }
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            message: message.into(),
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::BAD_REQUEST,
            Self::AuthRequired
            | Self::InvalidCredentials
            | Self::InvalidToken
            | Self::SessionExpired => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Configuration { .. } | Self::Store { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Serialization { .. } | Self::Transport { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub fn public_code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation",
            Self::AuthRequired => "auth_required",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidToken => "invalid_token",
            Self::SessionExpired => "session_expired",
            Self::Forbidden => "forbidden",
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::RateLimited { .. } => "rate_limited",
            Self::Configuration { .. } => "configuration",
            Self::Store { .. } => "store",
            Self::Serialization { .. } => "serialization",
            Self::Transport { .. } => "transport",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Validation { message }
            | Self::NotFound { message }
            | Self::Conflict { message } => message.clone(),
            Self::RateLimited {
                retry_after_seconds,
            } => format!("too many requests; retry after {retry_after_seconds} seconds"),
            Self::AuthRequired => "authentication is required".to_string(),
            Self::InvalidCredentials => "Email or password is incorrect".to_string(),
            Self::InvalidToken => "access token is invalid".to_string(),
            Self::SessionExpired => "the session has expired".to_string(),
            Self::Forbidden => "the current account cannot access this resource".to_string(),
            Self::Configuration { .. } => {
                "auth stack is not configured for this operation".to_string()
            }
            Self::Store { .. } => "auth storage is unavailable".to_string(),
            Self::Serialization { .. } | Self::Transport { .. } => {
                "auth stack request failed; check server logs".to_string()
            }
        }
    }

    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::Validation { .. }
                | Self::AuthRequired
                | Self::InvalidCredentials
                | Self::InvalidToken
                | Self::SessionExpired
                | Self::Forbidden
                | Self::NotFound { .. }
                | Self::Conflict { .. }
                | Self::RateLimited { .. }
        )
    }

    pub fn response_body(&self) -> AuthErrorResponse {
        AuthErrorResponse {
            error: AuthErrorPayload {
                code: self.public_code(),
                message: self.public_message(),
            },
        }
    }

    pub fn server_fn_error(&self) -> server_fn::ServerFnError {
        server_fn::ServerFnError::new(self.public_message())
    }

    #[cfg(all(feature = "spin-grpc", runtime_spin))]
    pub fn grpc_status(&self) -> tonic::Status {
        tonic::Status::new(self.grpc_code(), self.public_message())
    }

    #[cfg(all(feature = "spin-grpc", runtime_spin))]
    pub fn grpc_code(&self) -> tonic::Code {
        match self {
            Self::Validation { .. } => tonic::Code::InvalidArgument,
            Self::AuthRequired
            | Self::InvalidCredentials
            | Self::InvalidToken
            | Self::SessionExpired => tonic::Code::Unauthenticated,
            Self::Forbidden => tonic::Code::PermissionDenied,
            Self::NotFound { .. } => tonic::Code::NotFound,
            Self::Conflict { .. } => tonic::Code::Aborted,
            Self::RateLimited { .. } => tonic::Code::ResourceExhausted,
            Self::Configuration { .. } => tonic::Code::FailedPrecondition,
            Self::Store { .. } => tonic::Code::Unavailable,
            Self::Serialization { .. } | Self::Transport { .. } => tonic::Code::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_required_maps_to_unauthorized_status() {
        assert_eq!(
            AuthStackError::AuthRequired.http_status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn forbidden_maps_to_forbidden_status() {
        assert_eq!(
            AuthStackError::Forbidden.http_status(),
            StatusCode::FORBIDDEN
        );
    }
}
