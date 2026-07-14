#![allow(unused_imports)]
#![allow(dead_code)]

use std::sync::OnceLock;


use wasi_auth::authentication::jwt::JwksDocument;
use wasi_auth::authentication::Clock;
use wasi_auth::authorization::{
    AccessRequest, ActionName, Authorizer, MAX_BATCH_CHECKS, Resource, ResourceType,
};
use wasi_auth::cedar::{
    CedarError, CedarProvider, DEFAULT_APPLICATION_POLICY, DEFAULT_APPLICATION_POLICY_REVISION,
};
use wasi_auth::context::{
    AuthenticationAssurance, AuthorizationSnapshot, OrganizationId, PolicyRevision, Principal,
    RoleId, SessionId, UserId, VerifiedAuthContext, VerifiedRequestContext,
};
use wasi_auth::http::{
    AuthenticatedSession, Credential, CredentialAuthenticator, RoutePolicy, TrustedIngress,
    TrustedIngressConfig,
};

use super::*;
use crate::contracts::*;
use crate::error::{AuthStackError, AuthStackResult};


/// Resolve org id from slug or explicit id and assert membership + vault permission.
pub(crate) async fn require_vault_org(
    context: &VerifiedAuthContext,
    organization_id: Option<&str>,
    org_slug: Option<&str>,
    required_permission: &str,
) -> AuthStackResult<String> {
    let org_id = if let Some(slug) = org_slug.map(str::trim).filter(|s| !s.is_empty()) {
        crate::store::resolve_org_id_for_slug(slug).await?
    } else if let Some(id) = organization_id.map(str::trim).filter(|s| !s.is_empty()) {
        id.to_owned()
    } else {
        // Fall back to session-selected tenant (auto-bind first org if unset).
        let user_id = context.principal().user_id().as_str();
        let session = crate::auth_product::ensure_default_organization(
            context.session_id().as_str(),
            user_id,
        )
        .await?;
        session
            .tenant_id
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                AuthStackError::validation(
                    "create a workspace before using the secret vault",
                )
            })?
    };
    // Membership + role permission (vault.view | vault.manage | vault.reveal).
    let org = crate::auth_product::organization_for_session(
        context.session_id().as_str(),
        &org_id,
    )
    .await?;
    if !org
        .permissions
        .iter()
        .any(|p| p == required_permission)
    {
        // Soft-compat: if migration 0012 not applied yet, allow owner/admin by role.
        let legacy_ok = matches!(
            (required_permission, org.current_user_role.as_str()),
            ("vault.view", _)
                | ("vault.manage", "owner" | "admin")
                | ("vault.reveal", "owner" | "admin")
        ) && org.permissions.iter().all(|p| !p.starts_with("vault."));
        if !legacy_ok {
            return Err(AuthStackError::Forbidden);
        }
    }
    let user_id = context.principal().user_id().as_str();
    let _ = crate::store::migrate_legacy_user_secrets_to_org(user_id, &org_id).await;
    Ok(org_id)
}

pub async fn list_dashboard_secrets(
    organization_id: Option<String>,
    org_slug: Option<String>,
    auth: RequestAuth,
) -> AuthStackResult<Vec<SecretSummary>> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let org_id = require_vault_org(
        &context,
        organization_id.as_deref(),
        org_slug.as_deref(),
        "vault.view",
    )
    .await?;
    crate::store::list_secret_summaries(&org_id).await
}

pub async fn create_dashboard_secret(
    organization_id: Option<String>,
    org_slug: Option<String>,
    request: SecretCreateRequest,
    auth: RequestAuth,
) -> AuthStackResult<SecretSummary> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let org_id = require_vault_org(
        &context,
        organization_id.as_deref(),
        org_slug.as_deref(),
        "vault.manage",
    )
    .await?;
    crate::store::create_secret(&org_id, &request).await
}

pub async fn delete_dashboard_secret(
    organization_id: Option<String>,
    org_slug: Option<String>,
    secret_id: String,
    auth: RequestAuth,
) -> AuthStackResult<AcceptedResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let org_id = require_vault_org(
        &context,
        organization_id.as_deref(),
        org_slug.as_deref(),
        "vault.manage",
    )
    .await?;
    crate::store::delete_secret(&org_id, secret_id.trim()).await?;
    Ok(AcceptedResponse { accepted: true })
}

pub(crate) async fn vault_reveal_require_step_up() -> bool {
    let production = matches!(
        config_value("AUTH_PRODUCTION_MODE")
            .await
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    );
    match config_value("AUTH_VAULT_REVEAL_REQUIRE_STEP_UP")
        .await
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
    {
        Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on") => true,
        Some(v) if matches!(v.as_str(), "0" | "false" | "no" | "off") => false,
        // Default: require AAL2 in production; allow in local dev.
        _ => production,
    }
}

pub async fn reveal_dashboard_secret(
    organization_id: Option<String>,
    org_slug: Option<String>,
    secret_id: String,
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::SecretRevealResponse> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    if vault_reveal_require_step_up().await
        && context.assurance() != AuthenticationAssurance::Aal2
    {
        return Err(AuthStackError::Forbidden);
    }
    let org_id = require_vault_org(
        &context,
        organization_id.as_deref(),
        org_slug.as_deref(),
        "vault.reveal",
    )
    .await?;
    crate::store::reveal_secret(&org_id, secret_id.trim()).await
}

pub async fn seed_dashboard_demos(auth: RequestAuth) -> AuthStackResult<AcceptedResponse> {
    let (_user_id, org_id) = require_workspace_board(auth).await?;
    let _seeded = crate::store::seed_dashboard_demos(&org_id).await?;
    Ok(AcceptedResponse { accepted: true })
}

pub async fn migrate_workspace_legacy_data(
    request: crate::contracts::WorkspaceLegacyMigrateRequest,
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::WorkspaceLegacyMigrateReport> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let org_id = request.organization_id.trim().to_owned();
    if org_id.is_empty() {
        return Err(AuthStackError::validation("organization_id is required"));
    }
    // Owner/admin via vault.manage (or legacy role).
    let _ = require_vault_org(
        &context,
        Some(&org_id),
        None,
        "vault.manage",
    )
    .await?;
    let user_id = context.principal().user_id().as_str().to_owned();
    let dry_run = request.dry_run;

    let board_copied = if dry_run {
        // Report whether legacy keys exist without writing.
        false
    } else {
        crate::store::migrate_legacy_user_board_to_org(&user_id, &org_id).await?
    };

    let (secrets_copied, secret_rows, skipped, reenter) =
        crate::store::migrate_legacy_user_secrets_to_org_detailed(
            &user_id,
            &org_id,
            dry_run,
        )
        .await?;

    tracing::info!(
        user_id = %user_id,
        organization_id = %org_id,
        dry_run,
        board_copied,
        secrets_copied,
        secret_rows,
        skipped,
        "workspace legacy migrate"
    );

    let message = if dry_run {
        format!(
            "Dry run: would copy secrets rows={secret_rows}, reenter_required={}",
            reenter.len()
        )
    } else if board_copied || secrets_copied {
        "Legacy workspace data migrated into this organization.".to_owned()
    } else if !reenter.is_empty() {
        "Nothing copied automatically; re-enter secrets listed in reenter_required_keys.".to_owned()
    } else {
        "No legacy user data found (or destination already populated).".to_owned()
    };

    Ok(crate::contracts::WorkspaceLegacyMigrateReport {
        organization_id: org_id,
        dry_run,
        board_copied,
        secrets_copied,
        secret_rows_copied: secret_rows,
        secret_rows_skipped_reenter: skipped,
        reenter_required_keys: reenter,
        message,
    })
}

pub async fn resolve_workspace_vault_target(
    auth: RequestAuth,
) -> AuthStackResult<crate::contracts::OrganizationSummary> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let user_id = context.principal().user_id().as_str();
    let orgs = crate::auth_product::list_organizations(user_id)
        .await?
        .organizations;
    if orgs.is_empty() {
        return Err(AuthStackError::not_found("no workspace yet"));
    }
    let session = crate::auth_product::ensure_default_organization(
        context.session_id().as_str(),
        user_id,
    )
    .await?;
    if let Some(tid) = session
        .tenant_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(org) = orgs.iter().find(|o| o.organization_id == tid) {
            return Ok(org.clone());
        }
    }
    Ok(orgs.into_iter().next().expect("non-empty"))
}
