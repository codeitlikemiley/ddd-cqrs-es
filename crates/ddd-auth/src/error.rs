use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    Validation { message: String },
    AlreadyRegistered,
    UserDisabled,
    UserNotRegistered,
    InvalidProvider,
    InvalidToken,
    SessionExpired,
    SessionRevoked,
    PermissionDenied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthErrorClass {
    InvalidArgument,
    AlreadyExists,
    NotFound,
    Unauthenticated,
    PermissionDenied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthTransportMapping {
    pub http_status: u16,
    pub grpc_code: &'static str,
    pub server_fn_code: &'static str,
}

impl AuthError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    pub fn public_code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "validation",
            Self::AlreadyRegistered => "already_registered",
            Self::UserDisabled => "user_disabled",
            Self::UserNotRegistered => "user_not_registered",
            Self::InvalidProvider => "invalid_provider",
            Self::InvalidToken => "invalid_token",
            Self::SessionExpired => "session_expired",
            Self::SessionRevoked => "session_revoked",
            Self::PermissionDenied => "permission_denied",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Validation { message } => message.clone(),
            Self::AlreadyRegistered => "user is already registered".to_string(),
            Self::UserDisabled => "user is disabled".to_string(),
            Self::UserNotRegistered => "user is not registered".to_string(),
            Self::InvalidProvider => "auth provider is invalid".to_string(),
            Self::InvalidToken => "token is invalid".to_string(),
            Self::SessionExpired => "session is expired".to_string(),
            Self::SessionRevoked => "session is revoked".to_string(),
            Self::PermissionDenied => "permission denied".to_string(),
        }
    }

    pub fn class(&self) -> AuthErrorClass {
        match self {
            Self::Validation { .. } | Self::InvalidProvider => AuthErrorClass::InvalidArgument,
            Self::AlreadyRegistered => AuthErrorClass::AlreadyExists,
            Self::UserNotRegistered => AuthErrorClass::NotFound,
            Self::InvalidToken | Self::SessionExpired | Self::SessionRevoked => {
                AuthErrorClass::Unauthenticated
            }
            Self::UserDisabled | Self::PermissionDenied => AuthErrorClass::PermissionDenied,
        }
    }

    pub fn transport_mapping(&self) -> AuthTransportMapping {
        self.class().transport_mapping()
    }
}

impl AuthErrorClass {
    pub fn transport_mapping(self) -> AuthTransportMapping {
        match self {
            Self::InvalidArgument => AuthTransportMapping {
                http_status: 400,
                grpc_code: "InvalidArgument",
                server_fn_code: "validation",
            },
            Self::AlreadyExists => AuthTransportMapping {
                http_status: 409,
                grpc_code: "AlreadyExists",
                server_fn_code: "conflict",
            },
            Self::NotFound => AuthTransportMapping {
                http_status: 404,
                grpc_code: "NotFound",
                server_fn_code: "not_found",
            },
            Self::Unauthenticated => AuthTransportMapping {
                http_status: 401,
                grpc_code: "Unauthenticated",
                server_fn_code: "auth_required",
            },
            Self::PermissionDenied => AuthTransportMapping {
                http_status: 403,
                grpc_code: "PermissionDenied",
                server_fn_code: "forbidden",
            },
        }
    }
}

impl Display for AuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation { message } => f.write_str(message),
            Self::AlreadyRegistered => f.write_str("user is already registered"),
            Self::UserDisabled => f.write_str("user is disabled"),
            Self::UserNotRegistered => f.write_str("user is not registered"),
            Self::InvalidProvider => f.write_str("auth provider is invalid"),
            Self::InvalidToken => f.write_str("token is invalid"),
            Self::SessionExpired => f.write_str("session is expired"),
            Self::SessionRevoked => f.write_str("session is revoked"),
            Self::PermissionDenied => f.write_str("permission denied"),
        }
    }
}

impl Error for AuthError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_maps_to_invalid_argument_transport_codes() {
        let error = AuthError::validation("email is required");

        assert_eq!(error.class(), AuthErrorClass::InvalidArgument);
        assert_eq!(error.transport_mapping().http_status, 400);
        assert_eq!(error.transport_mapping().grpc_code, "InvalidArgument");
    }

    #[test]
    fn already_registered_maps_to_conflict_transport_codes() {
        let mapping = AuthError::AlreadyRegistered.transport_mapping();

        assert_eq!(mapping.http_status, 409);
        assert_eq!(mapping.grpc_code, "AlreadyExists");
        assert_eq!(mapping.server_fn_code, "conflict");
    }

    #[test]
    fn invalid_token_maps_to_unauthenticated_transport_codes() {
        let mapping = AuthError::InvalidToken.transport_mapping();

        assert_eq!(mapping.http_status, 401);
        assert_eq!(mapping.grpc_code, "Unauthenticated");
        assert_eq!(mapping.server_fn_code, "auth_required");
    }

    #[test]
    fn permission_denied_maps_to_forbidden_transport_codes() {
        let mapping = AuthError::PermissionDenied.transport_mapping();

        assert_eq!(mapping.http_status, 403);
        assert_eq!(mapping.grpc_code, "PermissionDenied");
        assert_eq!(mapping.server_fn_code, "forbidden");
    }
}
