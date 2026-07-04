//! Authentication primitives for `ddd_cqrs_es` applications.
//!
//! This crate intentionally keeps HTTP, gRPC, Leptos, Spin, and secret manager
//! behavior outside the core domain. Runtime applications own those adapters.
//!
//! # Audit metadata
//!
//! `AuthenticatedPrincipal` can build `ddd_cqrs_es::Metadata` for command
//! execution without coupling the domain to a specific transport.
//!
//! ```rust
//! use ddd_auth::{
//!     AuthProviderId, AuthenticatedPrincipal, SessionId, TenantId, UserId,
//!     AUTH_PROVIDER_METADATA_KEY,
//! };
//!
//! let principal = AuthenticatedPrincipal {
//!     user_id: Some(UserId::from("user:alice")),
//!     tenant_id: Some(TenantId::from("tenant:default")),
//!     session_id: Some(SessionId::from("session:1")),
//!     provider_id: Some(AuthProviderId::from("google")),
//!     ..AuthenticatedPrincipal::default()
//! };
//!
//! let metadata = principal.to_metadata_with_request("req-123", "corr-123");
//!
//! assert_eq!(metadata.actor_id.as_deref(), Some("user:alice"));
//! assert_eq!(metadata.tenant_id.as_deref(), Some("tenant:default"));
//! assert_eq!(metadata.request_id.as_deref(), Some("req-123"));
//! assert_eq!(
//!     metadata.headers.get(AUTH_PROVIDER_METADATA_KEY).map(String::as_str),
//!     Some("google"),
//! );
//! ```

mod domain;
mod error;
mod ids;
mod principal;
mod provider;
mod storage;
mod token;

pub use domain::{
    AuthProviderConfigAggregate, AuthProviderConfigCommand, AuthProviderConfigEvent,
    ExternalIdentity, ExternalIdentityCommand, ExternalIdentityEvent, PasskeyCredential,
    PasskeyCredentialCommand, PasskeyCredentialEvent, PasswordCredential,
    PasswordCredentialCommand, PasswordCredentialEvent, Session, SessionCommand, SessionEvent,
    SigningKeySet, SigningKeySetCommand, SigningKeySetEvent, SigningKeyState, SigningKeyStatus,
    User, UserCommand, UserEvent,
};
pub use error::{AuthError, AuthErrorClass, AuthTransportMapping};
pub use ids::{
    AuthProviderId, ExternalSubjectId, PasskeyCredentialId, SessionId, SigningKeyId, TenantId,
    UserId,
};
pub use principal::{
    AuthenticatedPrincipal, AUTH_PROVIDER_METADATA_KEY, AUTH_ROLES_METADATA_KEY,
    AUTH_SCOPES_METADATA_KEY, AUTH_SESSION_METADATA_KEY,
};
pub use provider::{AuthProviderConfig, OAuthProviderProfile};
pub use storage::{
    auth_read_model_contract, auth_stream_contract, AuthEventStreamContract, AuthReadModelContract,
    AUTH_EVENT_STREAMS, AUTH_EXTERNAL_IDENTITY_STREAM, AUTH_PASSKEY_CREDENTIAL_STREAM,
    AUTH_PASSWORD_CREDENTIAL_STREAM, AUTH_PROVIDER_CONFIG_STREAM, AUTH_READ_MODELS,
    AUTH_REFRESH_TOKEN_READ_MODEL, AUTH_SESSION_READ_MODEL, AUTH_SESSION_STREAM,
    AUTH_SIGNING_KEY_READ_MODEL, AUTH_SIGNING_KEY_STREAM, AUTH_STORAGE_VERSION,
    AUTH_TOKEN_GRANT_READ_MODEL, AUTH_USER_BY_EMAIL_READ_MODEL, AUTH_USER_READ_MODEL,
    AUTH_USER_STREAM,
};
pub use token::{
    jwks_key_by_id, reject_revoked_session, AccessTokenClaims, IdTokenClaims, JwksDocument, JwksKey,
};

#[cfg(feature = "jwt")]
pub use token::{
    access_token_key_id, decode_access_token, decode_id_token, encode_access_token, jwt_key_id,
};

#[cfg(feature = "jwt")]
pub use jsonwebtoken::{
    encode as encode_jwt, jwk::Jwk, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};

#[cfg(feature = "oauth")]
pub use oauth2::{CsrfToken, PkceCodeChallenge, PkceCodeVerifier};

#[cfg(feature = "oauth")]
pub use openidconnect::Nonce;

#[cfg(feature = "passkeys")]
pub mod passkeys {
    pub use passkey_auth::{
        Attachment, AuthSuccess, AuthenticationChallenge, AuthenticationResponse,
        AuthenticationState, Challenge, CredentialId, PasskeyCredential, RegistrationChallenge,
        RegistrationResponse, RegistrationState, Webauthn,
    };
}
