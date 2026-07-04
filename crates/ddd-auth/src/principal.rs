use crate::{AuthProviderId, SessionId, TenantId, UserId};
use ddd_cqrs_es::Metadata;
use std::collections::BTreeMap;

pub const AUTH_PROVIDER_METADATA_KEY: &str = "auth.provider_id";
pub const AUTH_SESSION_METADATA_KEY: &str = "auth.session_id";
pub const AUTH_SCOPES_METADATA_KEY: &str = "auth.scopes";
pub const AUTH_ROLES_METADATA_KEY: &str = "auth.roles";

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthenticatedPrincipal {
    pub user_id: Option<UserId>,
    pub tenant_id: Option<TenantId>,
    pub session_id: Option<SessionId>,
    pub provider_id: Option<AuthProviderId>,
    pub scopes: Vec<String>,
    pub roles: Vec<String>,
    pub claims: BTreeMap<String, String>,
}

impl AuthenticatedPrincipal {
    pub fn anonymous() -> Self {
        Self::default()
    }

    pub fn is_authenticated(&self) -> bool {
        self.user_id.is_some()
    }

    pub fn to_metadata(&self) -> Metadata {
        let mut metadata = Metadata::new();
        if let Some(user_id) = &self.user_id {
            metadata = metadata.with_actor_id(user_id.as_str());
        }
        if let Some(tenant_id) = &self.tenant_id {
            metadata = metadata.with_tenant_id(tenant_id.as_str());
        }
        if let Some(provider_id) = &self.provider_id {
            metadata = metadata.with_header(AUTH_PROVIDER_METADATA_KEY, provider_id.as_str());
        }
        if let Some(session_id) = &self.session_id {
            metadata = metadata.with_header(AUTH_SESSION_METADATA_KEY, session_id.as_str());
        }
        if !self.scopes.is_empty() {
            metadata = metadata.with_header(AUTH_SCOPES_METADATA_KEY, self.scopes.join(" "));
        }
        if !self.roles.is_empty() {
            metadata = metadata.with_header(AUTH_ROLES_METADATA_KEY, self.roles.join(" "));
        }
        metadata
    }

    pub fn to_metadata_with_request(
        &self,
        request_id: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Metadata {
        self.to_metadata()
            .with_request_id(request_id)
            .with_correlation_id(correlation_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_principal_metadata_sets_actor_and_tenant() {
        let principal = AuthenticatedPrincipal {
            user_id: Some(UserId::from("user:alice")),
            tenant_id: Some(TenantId::from("tenant:default")),
            ..AuthenticatedPrincipal::default()
        };

        let metadata = principal.to_metadata();

        assert_eq!(metadata.actor_id.as_deref(), Some("user:alice"));
        assert_eq!(metadata.tenant_id.as_deref(), Some("tenant:default"));
    }

    #[test]
    fn authenticated_principal_metadata_sets_request_context() {
        let principal = AuthenticatedPrincipal::default();

        let metadata = principal.to_metadata_with_request("req-1", "corr-1");

        assert_eq!(metadata.request_id.as_deref(), Some("req-1"));
        assert_eq!(metadata.correlation_id.as_deref(), Some("corr-1"));
    }

    #[test]
    fn authenticated_principal_metadata_sets_provider_and_session_headers() {
        let principal = AuthenticatedPrincipal {
            provider_id: Some(AuthProviderId::from("google")),
            session_id: Some(SessionId::from("session-1")),
            scopes: vec!["auth:session:read".to_string(), "auth:logout".to_string()],
            roles: vec!["owner".to_string()],
            ..AuthenticatedPrincipal::default()
        };

        let metadata = principal.to_metadata();

        assert_eq!(
            metadata
                .headers
                .get(AUTH_PROVIDER_METADATA_KEY)
                .map(String::as_str),
            Some("google")
        );
        assert_eq!(
            metadata
                .headers
                .get(AUTH_SESSION_METADATA_KEY)
                .map(String::as_str),
            Some("session-1")
        );
        assert_eq!(
            metadata
                .headers
                .get(AUTH_SCOPES_METADATA_KEY)
                .map(String::as_str),
            Some("auth:session:read auth:logout")
        );
        assert_eq!(
            metadata
                .headers
                .get(AUTH_ROLES_METADATA_KEY)
                .map(String::as_str),
            Some("owner")
        );
    }
}
