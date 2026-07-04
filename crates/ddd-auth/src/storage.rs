//! Stable storage contract names for authentication services.
//!
//! Runtime applications own the actual adapter and migration implementation.

/// Version marker for the first auth storage contract.
pub const AUTH_STORAGE_VERSION: &str = "auth.v1";

/// Event stream contract for an authentication aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthEventStreamContract {
    pub stream_name: &'static str,
    pub aggregate_type: &'static str,
    pub id_prefix: &'static str,
}

/// Read model contract for an authentication projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthReadModelContract {
    pub name: &'static str,
    pub primary_key: &'static str,
    pub purpose: &'static str,
}

pub const AUTH_USER_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.users",
    aggregate_type: "auth_user",
    id_prefix: "user",
};

pub const AUTH_PASSWORD_CREDENTIAL_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.password_credentials",
    aggregate_type: "auth_password_credential",
    id_prefix: "password_credential",
};

pub const AUTH_EXTERNAL_IDENTITY_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.external_identities",
    aggregate_type: "auth_external_identity",
    id_prefix: "external_identity",
};

pub const AUTH_PASSKEY_CREDENTIAL_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.passkey_credentials",
    aggregate_type: "auth_passkey_credential",
    id_prefix: "passkey_credential",
};

pub const AUTH_SESSION_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.sessions",
    aggregate_type: "auth_session",
    id_prefix: "session",
};

pub const AUTH_SIGNING_KEY_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.signing_keys",
    aggregate_type: "auth_signing_key_set",
    id_prefix: "signing_key",
};

pub const AUTH_PROVIDER_CONFIG_STREAM: AuthEventStreamContract = AuthEventStreamContract {
    stream_name: "auth.provider_configs",
    aggregate_type: "auth_provider_config",
    id_prefix: "auth_provider",
};

pub const AUTH_EVENT_STREAMS: &[AuthEventStreamContract] = &[
    AUTH_USER_STREAM,
    AUTH_PASSWORD_CREDENTIAL_STREAM,
    AUTH_EXTERNAL_IDENTITY_STREAM,
    AUTH_PASSKEY_CREDENTIAL_STREAM,
    AUTH_SESSION_STREAM,
    AUTH_SIGNING_KEY_STREAM,
    AUTH_PROVIDER_CONFIG_STREAM,
];

pub const AUTH_USER_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_users",
    primary_key: "user_id",
    purpose: "canonical user account projection",
};

pub const AUTH_USER_BY_EMAIL_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_users_by_email",
    primary_key: "tenant_id, normalized_email",
    purpose: "unique user lookup for email login and account linking",
};

pub const AUTH_EXTERNAL_IDENTITY_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_external_identities",
    primary_key: "tenant_id, provider_id, provider_subject",
    purpose: "OAuth/OIDC provider subject lookup and local account linking",
};

pub const AUTH_SESSION_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_sessions",
    primary_key: "session_id",
    purpose: "active session, refresh token, and revocation lookup",
};

pub const AUTH_REFRESH_TOKEN_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_refresh_token_hashes",
    primary_key: "tenant_id, token_hash",
    purpose: "refresh token rotation and replay detection",
};

pub const AUTH_SIGNING_KEY_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_jwks",
    primary_key: "key_id",
    purpose: "JWKS publication and token verification key lookup",
};

pub const AUTH_SIGNING_KEY_LIFECYCLE_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_signing_keys",
    primary_key: "tenant_id, kid",
    purpose: "active, next, retired, and revoked signing key lifecycle state",
};

pub const AUTH_PROVIDER_CONFIG_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_provider_configs",
    primary_key: "tenant_id, provider_id",
    purpose: "configured OAuth/OIDC provider metadata",
};

pub const AUTH_PASSKEY_CREDENTIAL_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_passkey_credentials",
    primary_key: "tenant_id, credential_id",
    purpose: "passkey registration and authentication lookup",
};

pub const AUTH_TOKEN_GRANT_READ_MODEL: AuthReadModelContract = AuthReadModelContract {
    name: "auth_token_grants",
    primary_key: "tenant_id, grant_id",
    purpose: "OAuth authorization code and device grant lifecycle",
};

pub const AUTH_READ_MODELS: &[AuthReadModelContract] = &[
    AUTH_USER_READ_MODEL,
    AUTH_USER_BY_EMAIL_READ_MODEL,
    AUTH_EXTERNAL_IDENTITY_READ_MODEL,
    AUTH_SESSION_READ_MODEL,
    AUTH_REFRESH_TOKEN_READ_MODEL,
    AUTH_SIGNING_KEY_READ_MODEL,
    AUTH_SIGNING_KEY_LIFECYCLE_READ_MODEL,
    AUTH_PROVIDER_CONFIG_READ_MODEL,
    AUTH_PASSKEY_CREDENTIAL_READ_MODEL,
    AUTH_TOKEN_GRANT_READ_MODEL,
];

/// Return the stream contract for a known aggregate type.
pub fn auth_stream_contract(aggregate_type: &str) -> Option<&'static AuthEventStreamContract> {
    AUTH_EVENT_STREAMS
        .iter()
        .find(|contract| contract.aggregate_type == aggregate_type)
}

/// Return the read model contract for a known projection name.
pub fn auth_read_model_contract(name: &str) -> Option<&'static AuthReadModelContract> {
    AUTH_READ_MODELS
        .iter()
        .find(|contract| contract.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn auth_stream_names_are_unique() {
        let mut names = BTreeSet::new();

        for contract in AUTH_EVENT_STREAMS {
            assert!(
                names.insert(contract.stream_name),
                "duplicate stream name: {}",
                contract.stream_name
            );
        }
    }

    #[test]
    fn auth_read_model_names_are_unique() {
        let mut names = BTreeSet::new();

        for contract in AUTH_READ_MODELS {
            assert!(
                names.insert(contract.name),
                "duplicate read model name: {}",
                contract.name
            );
        }
    }

    #[test]
    fn lookup_returns_auth_user_stream_contract() {
        let contract = auth_stream_contract("auth_user").expect("auth_user stream");

        assert_eq!(contract.stream_name, AUTH_USER_STREAM.stream_name);
        assert_eq!(contract.id_prefix, "user");
    }

    #[test]
    fn lookup_returns_password_credential_stream_contract() {
        let contract = auth_stream_contract("auth_password_credential")
            .expect("auth_password_credential stream");

        assert_eq!(
            contract.stream_name,
            AUTH_PASSWORD_CREDENTIAL_STREAM.stream_name
        );
        assert_eq!(contract.id_prefix, "password_credential");
    }

    #[test]
    fn lookup_returns_signing_key_set_stream_contract() {
        let contract =
            auth_stream_contract("auth_signing_key_set").expect("auth_signing_key_set stream");

        assert_eq!(contract.stream_name, AUTH_SIGNING_KEY_STREAM.stream_name);
        assert_eq!(contract.id_prefix, "signing_key");
    }
}
