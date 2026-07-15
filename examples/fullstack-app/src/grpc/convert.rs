#![allow(unused_imports)]
#![allow(dead_code)]

use tonic::{Request, Status};
use wasi_auth::authentication::jwt;

use super::*;

pub(crate) fn status_from_app_error(
    operation: &'static str,
    error: crate::error::AuthStackError,
) -> Status {
    if error.is_client_error() {
        tracing::warn!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "auth gRPC request rejected"
        );
    } else {
        tracing::error!(
            operation,
            error = %error,
            error_code = error.public_code(),
            grpc_code = ?error.grpc_code(),
            "auth gRPC request failed"
        );
    }
    error.grpc_status()
}

pub(crate) fn empty_to_option(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn request_auth<T>(request: &Request<T>) -> crate::application::RequestAuth {
    if let Some(context) = request
        .extensions()
        .get::<wasi_auth::context::VerifiedRequestContext>()
    {
        return crate::application::RequestAuth::from_verified(context.clone());
    }
    let metadata = request.metadata();
    crate::application::RequestAuth::from_parts(
        None,
        metadata_text(metadata, "authorization").and_then(|value| bearer_token(&value)),
        metadata_text(metadata, "x-request-id"),
    )
}

pub(crate) fn metadata_text(metadata: &tonic::metadata::MetadataMap, name: &str) -> Option<String> {
    metadata
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn bearer_token(value: &str) -> Option<String> {
    value
        .trim()
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

impl From<crate::contracts::AuthProviderSummary> for auth_proto::AuthProvider {
    fn from(value: crate::contracts::AuthProviderSummary) -> Self {
        Self {
            provider_id: value.provider_id,
            display_name: value.display_name,
            login_url: value.login_url,
            enabled: value.enabled,
        }
    }
}

impl From<crate::contracts::AuthCapabilities> for auth_proto::AuthCapabilitiesResponse {
    fn from(value: crate::contracts::AuthCapabilities) -> Self {
        Self {
            password_enabled: value.password_enabled,
            oauth_enabled: value.oauth_enabled,
            passkeys_enabled: value.passkeys_enabled,
            providers: value.providers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::OAuthStartResponse> for auth_proto::OAuthStartResponse {
    fn from(value: crate::contracts::OAuthStartResponse) -> Self {
        Self {
            provider_id: value.provider_id,
            authorization_url: value.authorization_url,
            state: value.state,
        }
    }
}

impl From<crate::contracts::LoginCompletionResponse> for auth_proto::LoginCompletionResponse {
    fn from(value: crate::contracts::LoginCompletionResponse) -> Self {
        Self {
            authenticated: value.authenticated,
            redirect_url: value.redirect_url,
            session_id: value.session_id.unwrap_or_default(),
            access_token: value.access_token.unwrap_or_default(),
            refresh_token: value.refresh_token.unwrap_or_default(),
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::PasswordResetStartResponse> for auth_proto::PasswordResetStartResponse {
    fn from(value: crate::contracts::PasswordResetStartResponse) -> Self {
        Self {
            accepted: value.accepted,
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::PasskeyStartResponse> for auth_proto::PasskeyStartResponse {
    fn from(value: crate::contracts::PasskeyStartResponse) -> Self {
        Self {
            challenge_id: value.challenge_id,
            public_key_options_json: value.public_key_options_json,
            redirect_url: value.redirect_url,
        }
    }
}

impl From<crate::contracts::SessionView> for auth_proto::SessionView {
    fn from(value: crate::contracts::SessionView) -> Self {
        Self {
            authenticated: value.authenticated,
            tenant_id: value.tenant_id.unwrap_or_default(),
            user_id: value.user_id.unwrap_or_default(),
            primary_email: value.primary_email.unwrap_or_default(),
            expires_at: value.expires_at.unwrap_or_default(),
            permissions: value.permissions,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            issued_at_unix_seconds: value.issued_at_unix_seconds.unwrap_or_default(),
            expires_at_unix_seconds: value.expires_at_unix_seconds.unwrap_or_default(),
            session_id: value.session_id.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::TokenRefreshResponse> for auth_proto::TokenRefreshResponse {
    fn from(value: crate::contracts::TokenRefreshResponse) -> Self {
        Self {
            access_token: value.access_token.unwrap_or_default(),
            refresh_token: value.refresh_token.unwrap_or_default(),
            expires_in_seconds: value.expires_in_seconds,
        }
    }
}

impl From<crate::contracts::TokenVerifyResponse> for auth_proto::TokenVerifyResponse {
    fn from(value: crate::contracts::TokenVerifyResponse) -> Self {
        Self {
            active: value.active,
            subject: value.subject,
            tenant_id: value.tenant_id.unwrap_or_default(),
            session_id: value.session_id.unwrap_or_default(),
            expires_at: value.expires_at,
            scopes: value.scopes,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            issued_at_unix_seconds: value.issued_at_unix_seconds,
        }
    }
}

impl From<crate::contracts::AccountSessionSummary> for auth_proto::AccountSession {
    fn from(value: crate::contracts::AccountSessionSummary) -> Self {
        Self {
            session_id: value.session_id,
            organization_id: value.organization_id.unwrap_or_default(),
            assurance: value.assurance,
            issued_at_ms: value.issued_at_ms,
            expires_at_ms: value.expires_at_ms,
            current: value.current,
        }
    }
}

impl From<crate::contracts::AccountSessionListResponse> for auth_proto::SessionListResponse {
    fn from(value: crate::contracts::AccountSessionListResponse) -> Self {
        Self {
            sessions: value.sessions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::LogoutResponse> for auth_proto::LogoutResponse {
    fn from(value: crate::contracts::LogoutResponse) -> Self {
        Self {
            redirect_url: value.redirect_url,
        }
    }
}

impl From<jwt::JwksDocument> for auth_proto::JwksDocument {
    fn from(value: jwt::JwksDocument) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<jwt::JwksKey> for auth_proto::JwksKey {
    fn from(value: jwt::JwksKey) -> Self {
        Self {
            kid: value.kid,
            kty: value.kty,
            alg: value.alg,
            r#use: value.use_,
            public_parameters: value.public_parameters.into_iter().collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyListResponse> for admin_proto::SigningKeyListResponse {
    fn from(value: crate::contracts::SigningKeyListResponse) -> Self {
        Self {
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeyRotateResponse> for admin_proto::SigningKeyRotateResponse {
    fn from(value: crate::contracts::SigningKeyRotateResponse) -> Self {
        Self {
            active_kid: value.active_kid,
            previous_kid: value.previous_kid.unwrap_or_default(),
            retired_previous: value.retired_previous,
            keys: value.keys.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SigningKeySummary> for admin_proto::SigningKey {
    fn from(value: crate::contracts::SigningKeySummary) -> Self {
        Self {
            kid: value.kid,
            alg: value.alg,
            status: value.status,
            active: value.active,
            source: value.source,
            created_at_ms: value.created_at_ms.unwrap_or_default(),
            activated_at_ms: value.activated_at_ms.unwrap_or_default(),
            retired_at_ms: value.retired_at_ms.unwrap_or_default(),
            revoked_at_ms: value.revoked_at_ms.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::AuthProviderSummary> for admin_proto::Provider {
    fn from(value: crate::contracts::AuthProviderSummary) -> Self {
        Self {
            provider_id: value.provider_id,
            display_name: value.display_name,
            login_url: value.login_url,
            enabled: value.enabled,
        }
    }
}

impl From<crate::contracts::OrganizationSummary> for organization_proto::Organization {
    fn from(value: crate::contracts::OrganizationSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            name: value.name,
            status: value.status,
            current_user_role: value.current_user_role,
            permissions: value.permissions,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::OrganizationListResponse>
    for organization_proto::OrganizationListResponse
{
    fn from(value: crate::contracts::OrganizationListResponse) -> Self {
        Self {
            organizations: value.organizations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::SessionView> for organization_proto::SessionView {
    fn from(value: crate::contracts::SessionView) -> Self {
        Self {
            authenticated: value.authenticated,
            organization_id: value.tenant_id.unwrap_or_default(),
            user_id: value.user_id.unwrap_or_default(),
            primary_email: value.primary_email.unwrap_or_default(),
            permissions: value.permissions,
            assurance: value.assurance,
            system_administrator: value.system_administrator,
            session_id: value.session_id.unwrap_or_default(),
            issued_at_unix_seconds: value.issued_at_unix_seconds.unwrap_or_default(),
            expires_at_unix_seconds: value.expires_at_unix_seconds.unwrap_or_default(),
        }
    }
}

impl From<crate::contracts::MembershipSummary> for organization_proto::Membership {
    fn from(value: crate::contracts::MembershipSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            user_id: value.user_id,
            primary_email: value.primary_email,
            role_id: value.role_id,
            status: value.status,
            joined_at_ms: value.joined_at_ms,
        }
    }
}

impl From<crate::contracts::MembershipListResponse> for organization_proto::MembershipListResponse {
    fn from(value: crate::contracts::MembershipListResponse) -> Self {
        Self {
            memberships: value.memberships.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::InvitationSummary> for organization_proto::Invitation {
    fn from(value: crate::contracts::InvitationSummary) -> Self {
        Self {
            invitation_id: value.invitation_id,
            organization_id: value.organization_id,
            email: value.email,
            role_id: value.role_id,
            status: value.status,
            expires_at_ms: value.expires_at_ms,
        }
    }
}

impl From<crate::contracts::InvitationListResponse> for organization_proto::InvitationListResponse {
    fn from(value: crate::contracts::InvitationListResponse) -> Self {
        Self {
            invitations: value.invitations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::RoleSummary> for organization_proto::Role {
    fn from(value: crate::contracts::RoleSummary) -> Self {
        Self {
            organization_id: value.organization_id,
            role_id: value.role_id,
            name: value.name,
            built_in: value.built_in,
            permissions: value.permissions,
        }
    }
}

impl From<crate::contracts::RoleListResponse> for organization_proto::RoleListResponse {
    fn from(value: crate::contracts::RoleListResponse) -> Self {
        Self {
            roles: value.roles.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::AdminUserSummary> for admin_proto::User {
    fn from(value: crate::contracts::AdminUserSummary) -> Self {
        Self {
            user_id: value.user_id,
            primary_email: value.primary_email,
            disabled: value.disabled,
            email_verified: value.email_verified,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::AdminUserListResponse> for admin_proto::UserListResponse {
    fn from(value: crate::contracts::AdminUserListResponse) -> Self {
        Self {
            users: value.users.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::PolicyVersionSummary> for admin_proto::PolicyVersion {
    fn from(value: crate::contracts::PolicyVersionSummary) -> Self {
        Self {
            version_id: value.version_id,
            status: value.status,
            policy_hash: value.policy_hash,
            published_by: value.published_by,
            created_at_ms: value.created_at_ms,
        }
    }
}

impl From<crate::contracts::PolicyVersionListResponse> for admin_proto::PolicyVersionListResponse {
    fn from(value: crate::contracts::PolicyVersionListResponse) -> Self {
        Self {
            versions: value.versions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::HealthStatusResponse> for admin_proto::HealthResponse {
    fn from(value: crate::contracts::HealthStatusResponse) -> Self {
        Self {
            status: value.status,
            storage_backend: value.storage_backend,
            mail_transport: value.mail_transport,
            authorization_provider: value.authorization_provider,
            production_mode: value.production_mode,
        }
    }
}

impl From<crate::contracts::AuditEventSummary> for audit_proto::AuditEvent {
    fn from(value: crate::contracts::AuditEventSummary) -> Self {
        Self {
            sequence: value.sequence,
            organization_id: value.organization_id.unwrap_or_default(),
            actor_user_id: value.actor_user_id,
            action: value.action,
            target_type: value.target_type,
            target_id: value.target_id,
            outcome: value.outcome,
            recorded_at_ms: value.recorded_at_ms,
        }
    }
}

impl From<crate::contracts::AuditEventListResponse> for audit_proto::AuditEventListResponse {
    fn from(value: crate::contracts::AuditEventListResponse) -> Self {
        Self {
            events: value.events.into_iter().map(Into::into).collect(),
            next_cursor: value.next_cursor,
        }
    }
}

impl From<authorization_proto::CheckRequest> for crate::contracts::AuthorizationCheckRequest {
    fn from(value: authorization_proto::CheckRequest) -> Self {
        Self {
            action: value.action,
            resource_type: value.resource_type,
            resource_id: value.resource_id,
            organization_id: empty_to_option(value.organization_id),
        }
    }
}

impl From<crate::contracts::AuthorizationCheckResponse> for authorization_proto::CheckResponse {
    fn from(value: crate::contracts::AuthorizationCheckResponse) -> Self {
        Self {
            allowed: value.allowed,
            reason: value.reason,
            policy_revision: value.policy_revision,
            consistency_token: value.consistency_token.unwrap_or_default(),
            resource_revision: value.resource_revision,
        }
    }
}

impl From<crate::contracts::AuthorizationBatchCheckResponse>
    for authorization_proto::BatchCheckResponse
{
    fn from(value: crate::contracts::AuthorizationBatchCheckResponse) -> Self {
        Self {
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::contracts::AuthorizationCapabilitiesResponse>
    for authorization_proto::CapabilitiesResponse
{
    fn from(value: crate::contracts::AuthorizationCapabilitiesResponse) -> Self {
        Self {
            provider: value.provider,
            batch_check: value.batch_check,
            list_resources: value.list_resources,
            consistency_tokens: value.consistency_tokens,
            max_batch_checks: value.max_batch_checks,
        }
    }
}
