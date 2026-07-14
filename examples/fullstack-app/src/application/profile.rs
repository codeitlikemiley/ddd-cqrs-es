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


pub async fn get_account_profile(auth: RequestAuth) -> AuthStackResult<ProfileView> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let user_id = context.principal().user_id().as_str().to_owned();
    let email = crate::auth_product::get_session(Some(context.session_id().as_str()))
        .await
        .ok()
        .and_then(|session| session.primary_email);
    crate::store::get_profile_for_user(&user_id, email).await
}

pub async fn update_account_profile(
    request: ProfileUpdateRequest,
    auth: RequestAuth,
) -> AuthStackResult<ProfileView> {
    let (context, _) = verified_context_and_permissions(auth, false).await?;
    let user_id = context.principal().user_id().as_str().to_owned();
    let email = crate::auth_product::get_session(Some(context.session_id().as_str()))
        .await
        .ok()
        .and_then(|session| session.primary_email);
    crate::store::update_profile_for_user(&user_id, email, request).await
}

pub async fn get_public_profile(username: String) -> AuthStackResult<PublicProfileView> {
    crate::store::get_public_profile_by_username(&username).await
}
