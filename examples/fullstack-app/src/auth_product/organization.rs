//! Thin Spin runtime adapter for product workflows owned by `wasi-auth`.
#![allow(unused_imports)]
#![allow(dead_code)]

use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use sha2::{Digest, Sha256};
#[cfg(feature = "mail-capture")]
use wasi_auth::mail::{EmailKind, Recipient};
#[cfg(feature = "mail-capture")]
use wasi_auth::postgres::outbox::{MailOutboxWorker, PublicBaseUrl};
use wasi_auth::{
    authentication::jwt::JwksDocument,
    authentication::mfa::TotpConfig,
    authentication::passkeys::Attachment as PasskeyAttachment,
    authentication::{Clock, RandomSource},
    context::{AuthenticationAssurance, RequestId, SessionId, UserId},
    http::{AuthenticatedSession, TrustedContextCodec},
    postgres::workflows::{
        Argon2Policy, EmailVerificationError, EmailVerificationRequest,
        EmailVerificationResendRequest as ProductEmailVerificationResendRequest,
        EmailVerificationService, OutboxSealingKey,
        PasswordChangeRequest as ProductPasswordChangeRequest, PasswordLoginError,
        PasswordLoginRequest, PasswordLoginService, PasswordRegistrationError,
        PasswordRegistrationRequest, PasswordRegistrationService,
        PasswordResetCompleteRequest as ProductPasswordResetCompleteRequest, PasswordResetError,
        PasswordResetService, PasswordResetStartRequest as ProductPasswordResetStartRequest,
    },
    postgres::{
        PostgresAuthStore, PostgresStoreError,
        flows::FlowSealingKey,
        management::{
            AdminUserRecord, AuditEventRecord, InvitationRecord, InvitationService,
            ManagementError, MembershipRecord, ORGANIZATION_PERMISSION_CATALOG,
            OrganizationAccessModel, OrganizationManagementService, RoleRecord,
            UpsertRoleRequest as ProductUpsertRoleRequest,
        },
        mfa::{MfaKeyMaterial, MfaService, MfaServiceError},
        oauth::{
            OAuthFlowService, OAuthProviderService, OAuthProviderServiceError, OAuthServiceConfig,
            OAuthServiceError, PendingOAuthFlow, VerifiedOAuthIdentity,
        },
        organizations::{
            CreateOrganizationRequest, OrganizationError, OrganizationRecord, OrganizationService,
        },
        passkeys::{
            PasskeyConfigurationError, PasskeyService, PasskeyServiceConfig, PasskeyServiceError,
        },
        policy::{
            ActivePolicyBundle, PolicyBundleLoadError, PolicyBundleRecord, PolicyBundleService,
            PolicyBundleServiceError,
        },
        rate_limits::{RateLimitError, RateLimitService},
        sessions::{SessionService, SessionServiceError},
        signing::{SigningKeyRecord, SigningKeyService, SigningKeyServiceError},
        spin::{SpinPostgresError, SpinPostgresTransport},
        tokens::{
            AccessTokenVerifier, JwtKeyRing, RefreshSealingKey, TokenService, TokenServiceConfig,
            TokenServiceError, VerifiedAccessToken,
        },
    },
};

use crate::{
    contracts::{
        AccountSessionListResponse, AccountSessionSummary, AdminUserListResponse, AdminUserSummary,
        AuditEventListResponse, AuditEventSummary, AuthProviderSummary, CapturedMailResponse,
        EmailPasswordLoginRequest, EmailPasswordRegisterRequest, EmailVerificationCompleteRequest,
        InvitationListResponse, InvitationSummary, LoginCompletionResponse, LogoutResponse,
        MembershipListResponse, MembershipSummary, MfaEnrollConfirmResponse,
        MfaEnrollStartResponse, MfaStatusResponse, OrganizationListResponse, OrganizationSummary,
        PasskeyStartResponse, PasswordResetCompleteRequest, PasswordResetStartRequest,
        PasswordResetStartResponse, PolicyVersionListResponse, PolicyVersionSummary,
        RoleListResponse, RoleSummary, SessionView, SigningKeyListResponse,
        SigningKeyRotateResponse, SigningKeySummary, TokenRefreshResponse, TokenVerifyResponse,
    },
    error::{AuthStackError, AuthStackResult},
};

use super::*;

pub async fn list_organizations(user_id: &str) -> AuthStackResult<OrganizationListResponse> {
    let user_id = UserId::new(user_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let organizations = OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .list(&user_id)
        .await
        .map_err(map_organization_error)?;
    let mut summaries = Vec::with_capacity(organizations.len());
    for record in organizations {
        summaries.push(organization_summary_with_slug(record).await);
    }
    Ok(OrganizationListResponse {
        organizations: summaries,
    })
}

pub async fn create_organization(
    name: &str,
    slug: &str,
    session_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id =
        SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    let slug = slug.trim().to_ascii_lowercase();
    let name_key =
        URL_SAFE_NO_PAD.encode(Sha256::digest(format!("{}:{slug}", name.trim()).as_bytes()));
    let organization = OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .create(CreateOrganizationRequest {
            idempotency_key: format!("create-organization:{}:{name_key}", session_id.as_str()),
            session_id,
            name: name.to_owned(),
            slug: slug.clone(),
            request_id: request_id("create-organization")?,
        })
        .await
        .map_err(map_organization_error)?;
    let summary = organization_summary(organization);
    // Dual-write slug to Spin KV for resolve fallback during transition.
    let _ = crate::store::register_org_slug(&summary.organization_id, &summary.slug).await;
    Ok(summary)
}

pub async fn select_organization(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<SessionView> {
    let session_id =
        SessionId::new(session_id.to_owned()).map_err(|_| AuthStackError::AuthRequired)?;
    OrganizationService::new(store().await?, RuntimeClock, RuntimeRandom)
        .select(
            &session_id,
            organization_id,
            &request_id("select-organization")?,
        )
        .await
        .map_err(map_organization_error)?;
    get_session(Some(session_id.as_str())).await
}

/// If the session has no selected workspace, select the first membership
/// (oldest by `created_at` — the product default until the user switches).
///
pub async fn ensure_default_organization(
    session_id: &str,
    user_id: &str,
) -> AuthStackResult<SessionView> {
    let session = get_session(Some(session_id)).await?;
    if session
        .tenant_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(session);
    }
    let organizations = list_organizations(user_id).await?.organizations;
    let Some(default_org) = organizations.into_iter().next() else {
        return Ok(session);
    };
    select_organization(session_id, &default_org.organization_id).await
}

/// Binds the oldest active membership before issuing tokens for a new session.
pub async fn bind_default_organization_for_session(session_id: &str) -> AuthStackResult<()> {
    let view = get_session(Some(session_id)).await?;
    let user_id = view
        .user_id
        .as_deref()
        .ok_or(AuthStackError::AuthRequired)?;
    ensure_default_organization(session_id, user_id).await?;
    Ok(())
}

pub async fn organization_for_session(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .organization(&session_id, organization_id)
        .await
        .map(organization_summary)
        .map_err(map_management_error)
}

pub async fn update_organization(
    session_id: &str,
    organization_id: &str,
    name: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .update_organization(
            &session_id,
            organization_id,
            name,
            &request_id("update-organization")?,
        )
        .await
        .map(organization_summary)
        .map_err(map_management_error)
}

pub async fn list_memberships(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<MembershipListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let memberships = management_service()
        .await?
        .list_memberships(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(MembershipListResponse {
        memberships: memberships.into_iter().map(membership_summary).collect(),
    })
}

pub async fn create_invitation(
    session_id: &str,
    organization_id: &str,
    email: &str,
    role_id: &str,
) -> AuthStackResult<InvitationSummary> {
    let session_id = bounded_session_id(session_id)?;
    let invitation = InvitationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        outbox_key().await?,
    )
    .with_transactional_mail_config(transactional_mail_config().await?)
    .create(
        &session_id,
        organization_id,
        email,
        role_id,
        &request_id("create-invitation")?,
    )
    .await
    .map_err(map_management_error)?;
    Ok(invitation_summary(invitation))
}

pub async fn list_invitations(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<InvitationListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let invitations = management_service()
        .await?
        .list_invitations(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(InvitationListResponse {
        invitations: invitations.into_iter().map(invitation_summary).collect(),
    })
}

pub async fn accept_invitation(
    session_id: &str,
    token: &str,
) -> AuthStackResult<OrganizationSummary> {
    let session_id = bounded_session_id(session_id)?;
    InvitationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        outbox_key().await?,
    )
    .accept(&session_id, token, &request_id("accept-invitation")?)
    .await
    .map(organization_summary)
    .map_err(map_management_error)
}

pub async fn revoke_invitation(
    session_id: &str,
    organization_id: &str,
    invitation_id: &str,
) -> AuthStackResult<InvitationSummary> {
    let session_id = bounded_session_id(session_id)?;
    InvitationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        outbox_key().await?,
    )
    .revoke(
        &session_id,
        organization_id,
        invitation_id,
        &request_id("revoke-invitation")?,
    )
    .await
    .map(invitation_summary)
    .map_err(map_management_error)
}

pub async fn resend_invitation(
    session_id: &str,
    organization_id: &str,
    invitation_id: &str,
) -> AuthStackResult<InvitationSummary> {
    let session_id = bounded_session_id(session_id)?;
    InvitationService::new(
        store().await?,
        RuntimeClock,
        RuntimeRandom,
        outbox_key().await?,
    )
    .with_transactional_mail_config(transactional_mail_config().await?)
    .resend(
        &session_id,
        organization_id,
        invitation_id,
        &request_id("resend-invitation")?,
    )
    .await
    .map(invitation_summary)
    .map_err(map_management_error)
}

pub async fn list_roles(
    session_id: &str,
    organization_id: &str,
) -> AuthStackResult<RoleListResponse> {
    let session_id = bounded_session_id(session_id)?;
    let roles = management_service()
        .await?
        .list_roles(&session_id, organization_id)
        .await
        .map_err(map_management_error)?;
    Ok(RoleListResponse {
        roles: roles.into_iter().map(role_summary).collect(),
    })
}

pub async fn upsert_role(
    session_id: &str,
    organization_id: &str,
    role_id: &str,
    name: &str,
    permissions: Vec<String>,
) -> AuthStackResult<RoleSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .upsert_role(ProductUpsertRoleRequest {
            session_id,
            organization_id: organization_id.to_owned(),
            role_id: role_id.to_owned(),
            name: name.to_owned(),
            permissions,
            request_id: request_id("upsert-role")?,
        })
        .await
        .map(role_summary)
        .map_err(map_management_error)
}

pub async fn delete_role(
    session_id: &str,
    organization_id: &str,
    role_id: &str,
) -> AuthStackResult<()> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .delete_role(
            &session_id,
            organization_id,
            role_id,
            &request_id("delete-role")?,
        )
        .await
        .map_err(map_management_error)
}

pub async fn assign_role(
    session_id: &str,
    organization_id: &str,
    user_id: &str,
    role_id: &str,
) -> AuthStackResult<MembershipSummary> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .assign_role(
            &session_id,
            organization_id,
            user_id,
            role_id,
            &request_id("assign-role")?,
        )
        .await
        .map(membership_summary)
        .map_err(map_management_error)
}

pub async fn remove_member(
    session_id: &str,
    organization_id: &str,
    user_id: &str,
) -> AuthStackResult<()> {
    let session_id = bounded_session_id(session_id)?;
    management_service()
        .await?
        .remove_member(
            &session_id,
            organization_id,
            user_id,
            &request_id("remove-member")?,
        )
        .await
        .map_err(map_management_error)
}

pub fn organization_permission_catalog() -> Vec<String> {
    ORGANIZATION_PERMISSION_CATALOG
        .iter()
        .map(|permission| (*permission).to_owned())
        .collect()
}

/// Custom-role-eligible permission options with labels from the product access model.
pub fn organization_permission_options() -> Vec<crate::contracts::PermissionOption> {
    OrganizationAccessModel::product_default()
        .definitions()
        .iter()
        .filter(|definition| definition.custom_role_eligible)
        .map(|definition| crate::contracts::PermissionOption {
            id: definition.id.to_owned(),
            label: definition.label.to_owned(),
            group: definition.group.to_owned(),
        })
        .collect()
}
