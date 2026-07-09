use ddd_auth::JwksDocument;
use ddd_authz::{
    AuthorizationModel, AuthzContext, Evaluator, ObjectRef, ObjectType, Relation,
    RelationDefinition, SubjectRef, TenantRef,
};

use crate::contracts::{
    AuthCapabilities, AuthProviderSummary, AuthzBatchCheckRequest, AuthzBatchCheckResponse,
    AuthzCheckRequest, AuthzCheckResponse, AuthzExpandRequest, AuthzExpandResponse,
    AuthzListObjectsRequest, AuthzListObjectsResponse, AuthzModelReadResponse,
    AuthzModelRef, AuthzModelWriteRequest, AuthzModelWriteResponse, EmailPasswordLoginRequest,
    EmailPasswordRegisterRequest, CsrfTokenResponse, LoginCompletionResponse, LogoutResponse,
    OAuthCallbackRequest, OAuthStartResponse, PasskeyStartRequest, PasskeyStartResponse, PasskeyVerifyRequest,
    PasswordResetCompleteRequest, PasswordResetStartRequest, PasswordResetStartResponse,
    RelationshipTupleWriteRequest, RelationshipTupleWriteResponse, SessionView,
    SigningKeyListResponse, SigningKeyRotateRequest, SigningKeyRotateResponse,
    StorageProjectionRunResponse, StorageStatusResponse, TokenRefreshResponse, TokenVerifyRequest,
    TokenVerifyResponse,
};
use crate::error::{AuthStackError, AuthStackResult};

const DEFAULT_PASSWORD_MIN_LENGTH: usize = 8;

#[derive(Clone, Debug, Default)]
pub struct RequestAuth {
    pub session_id: Option<String>,
    pub access_token: Option<String>,
    pub admin_token: Option<String>,
}

impl RequestAuth {
    pub fn from_parts(
        session_id: Option<String>,
        access_token: Option<String>,
        admin_token: Option<String>,
    ) -> Self {
        Self {
            session_id,
            access_token,
            admin_token,
        }
    }
}

pub async fn auth_capabilities() -> AuthStackResult<AuthCapabilities> {
    let password_enabled = feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await;
    let oauth_enabled = feature_enabled("AUTH_ENABLE_OAUTH", false).await;
    let passkeys_enabled = feature_enabled("AUTH_ENABLE_PASSKEYS", false).await;
    let providers = if oauth_enabled {
        list_credentialed_auth_providers().await?
    } else {
        Vec::new()
    };

    Ok(AuthCapabilities {
        password_enabled,
        oauth_enabled: oauth_enabled && !providers.is_empty(),
        passkeys_enabled,
        providers,
    })
}

pub async fn list_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Ok(Vec::new());
    }
    list_credentialed_auth_providers().await
}

pub async fn register_email_password(
    request: EmailPasswordRegisterRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_email_password_register(&request, password_min_length().await)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::register_email_password(&request, &redirect_url).await?;
    catch_up_storage_after_write("register_email_password").await;
    Ok(response)
}

pub async fn login_email_password(
    request: EmailPasswordLoginRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_email_password_login(&request)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::login_email_password(&request, &redirect_url).await?;
    catch_up_storage_after_write("login_email_password").await;
    Ok(response)
}

pub async fn start_password_reset(
    request: PasswordResetStartRequest,
) -> AuthStackResult<PasswordResetStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_password_reset_start(&request)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::start_password_reset(&request, &redirect_url).await?;
    catch_up_storage_after_write("start_password_reset").await;
    Ok(response)
}

pub async fn complete_password_reset(
    request: PasswordResetCompleteRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSWORD_LOGIN", true).await {
        return Err(AuthStackError::configuration(
            "email/password login is disabled",
        ));
    }
    validate_password_reset_complete(&request, password_min_length().await)?;
    let redirect_url = safe_redirect_or_default(request.redirect_url.clone());
    let response = crate::store::complete_password_reset(&request, &redirect_url).await?;
    catch_up_storage_after_write("complete_password_reset").await;
    Ok(response)
}

pub async fn get_current_session_for(session_id: Option<String>) -> AuthStackResult<SessionView> {
    crate::store::get_session(session_id.as_deref()).await
}

pub async fn csrf_token_for_session(session_id: Option<String>) -> AuthStackResult<CsrfTokenResponse> {
    require_authenticated_route_for(session_id.clone()).await?;
    let session_id = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AuthStackError::AuthRequired)?;
    Ok(CsrfTokenResponse {
        token: crate::store::csrf_token_for_session(session_id).await?,
    })
}

pub async fn validate_csrf_token_for_session(
    session_id: Option<String>,
    csrf_token: Option<String>,
) -> AuthStackResult<()> {
    let Some(session_id) = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::AuthRequired);
    };
    require_authenticated_route_for(Some(session_id.to_string())).await?;
    let expected = crate::store::csrf_token_for_session(session_id).await?;
    let Some(candidate) = csrf_token.as_deref().map(str::trim).filter(|value| !value.is_empty())
    else {
        return Err(AuthStackError::validation("x-csrf-token is required"));
    };
    if expected != candidate {
        return Err(AuthStackError::Forbidden);
    }
    Ok(())
}

pub async fn session_cookie_secure_enabled() -> bool {
    if let Some(value) = config_value("AUTH_COOKIE_SECURE")
        .await
        .filter(|value| !value.trim().is_empty())
    {
        return truthy(&value);
    }
    feature_enabled("AUTH_PRODUCTION_MODE", false).await
}

pub fn session_cookie_header_value(
    session_id: &str,
    max_age_seconds: Option<u64>,
    secure: bool,
) -> String {
    let mut value = format!("ddd_auth_session={session_id}; Path=/; HttpOnly; SameSite=Lax");
    if let Some(max_age_seconds) = max_age_seconds {
        value.push_str(&format!("; Max-Age={max_age_seconds}"));
    }
    if secure {
        value.push_str("; Secure");
    }
    value
}

pub fn expired_session_cookie_header_value(secure: bool) -> String {
    session_cookie_header_value("", Some(0), secure)
}

pub async fn require_authenticated_route_for(
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = get_current_session_for(session_id).await?;
    if session.authenticated {
        Ok(session)
    } else {
        Err(AuthStackError::AuthRequired)
    }
}

pub async fn require_authorized_route_for(
    permission: &str,
    session_id: Option<String>,
) -> AuthStackResult<SessionView> {
    let session = require_authenticated_route_for(session_id).await?;
    if session.permissions.iter().any(|value| value == permission) {
        Ok(session)
    } else {
        Err(AuthStackError::Forbidden)
    }
}

pub async fn require_permission_for(
    permission: &str,
    auth: RequestAuth,
) -> AuthStackResult<SessionView> {
    if let Some(admin_token) = auth.admin_token.as_deref().filter(|value| !value.trim().is_empty())
    {
        crate::store::validate_admin_token(Some(admin_token)).await?;
        return Ok(admin_session_view(permission));
    }

    if let Some(access_token) = auth.access_token.as_deref().filter(|value| !value.trim().is_empty())
    {
        let verified = verify_access_token(TokenVerifyRequest {
            access_token: access_token.to_string(),
        })
        .await?;
        if verified.scopes.iter().any(|scope| scope == permission) {
            return Ok(SessionView {
                authenticated: true,
                tenant_id: verified.tenant_id,
                user_id: Some(verified.subject),
                primary_email: None,
                expires_at: None,
                permissions: verified.scopes,
            });
        }
        return Err(AuthStackError::Forbidden);
    }

    require_authorized_route_for(permission, auth.session_id).await
}

fn admin_session_view(permission: &str) -> SessionView {
    SessionView {
        authenticated: true,
        tenant_id: Some("tenant:default".to_string()),
        user_id: Some("operator:admin-token".to_string()),
        primary_email: None,
        expires_at: None,
        permissions: vec![permission.to_string()],
    }
}

pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> AuthStackResult<OAuthStartResponse> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Err(AuthStackError::configuration(
            "OAuth login is disabled; set AUTH_ENABLE_OAUTH=true and provider credentials to enable it",
        ));
    }
    validate_provider_id(&provider_id)?;
    let redirect_url = safe_redirect_or_default(redirect_url);
    ensure_oauth_provider_ready(&provider_id).await?;
    let state = crate::store::create_oauth_grant(&provider_id, &redirect_url).await?;
    catch_up_storage_after_write("start_oauth_login").await;

    if development_oauth_callback_bypass_enabled().await {
        return Ok(OAuthStartResponse {
            provider_id: provider_id.clone(),
            authorization_url: development_oauth_callback_url(&provider_id, &state, &redirect_url),
            state,
        });
    }

    Ok(OAuthStartResponse {
        provider_id: provider_id.clone(),
        authorization_url: crate::oauth::authorization_url(&provider_id, &state, &redirect_url)
            .await?,
        state,
    })
}

pub async fn complete_oauth_callback(
    request: OAuthCallbackRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_OAUTH", false).await {
        return Err(AuthStackError::configuration(
            "OAuth login is disabled; set AUTH_ENABLE_OAUTH=true and provider credentials to enable it",
        ));
    }
    validate_provider_id(&request.provider_id)?;
    if request
        .code
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AuthStackError::validation(
            "OAuth callback code is required",
        ));
    }
    if request
        .state
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(AuthStackError::validation(
            "OAuth callback state is required",
        ));
    }
    ensure_oauth_provider_ready(&request.provider_id).await?;

    let code = request.code.as_deref().unwrap_or_default().trim();
    let state = request.state.as_deref().unwrap_or_default().trim();
    let grant = crate::store::consume_oauth_grant(&request.provider_id, state).await?;
    let response = if development_oauth_callback_bypass_enabled().await {
        if code != "development-oauth-code" {
            return Err(AuthStackError::validation(
                "OAuth development callback code is invalid",
            ));
        }
        crate::store::issue_oauth_development_session(&grant).await?
    } else {
        let identity =
            crate::oauth::complete_authorization_code(&request.provider_id, code, &grant).await?;
        crate::store::issue_oauth_session(&identity, &grant.redirect_url).await?
    };
    catch_up_storage_after_write("complete_oauth_callback").await;
    Ok(response)
}

pub async fn start_passkey_login(
    request: PasskeyStartRequest,
) -> AuthStackResult<PasskeyStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey login is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    if let Some(email) = request.email.as_deref() {
        validate_optional_email(email)?;
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let response =
        crate::store::create_passkey_challenge("login", request.email, &redirect_url).await?;
    catch_up_storage_after_write("start_passkey_login").await;
    Ok(response)
}

pub async fn start_passkey_registration(
    request: PasskeyStartRequest,
) -> AuthStackResult<PasskeyStartResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey registration is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    if let Some(email) = request.email.as_deref() {
        validate_optional_email(email)?;
    }
    let redirect_url = safe_redirect_or_default(request.redirect_url);
    let response =
        crate::store::create_passkey_challenge("registration", request.email, &redirect_url)
            .await?;
    catch_up_storage_after_write("start_passkey_registration").await;
    Ok(response)
}

pub async fn verify_passkey_login(
    request: PasskeyVerifyRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey login is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    validate_passkey_verify_request(&request)?;
    let response = crate::store::verify_passkey_login(
        &request.challenge_id,
        &request.credential_json,
        request.redirect_url,
    )
    .await?;
    catch_up_storage_after_write("verify_passkey_login").await;
    Ok(response)
}

pub async fn verify_passkey_registration(
    request: PasskeyVerifyRequest,
) -> AuthStackResult<LoginCompletionResponse> {
    if !feature_enabled("AUTH_ENABLE_PASSKEYS", false).await {
        return Err(AuthStackError::configuration(
            "passkey registration is disabled; set AUTH_ENABLE_PASSKEYS=true to enable it",
        ));
    }
    validate_passkey_verify_request(&request)?;
    let response = crate::store::verify_passkey_registration(
        &request.challenge_id,
        &request.credential_json,
        request.redirect_url,
    )
    .await?;
    catch_up_storage_after_write("verify_passkey_registration").await;
    Ok(response)
}

pub async fn refresh_token_for(
    session_id: Option<String>,
    refresh_token: Option<String>,
) -> AuthStackResult<TokenRefreshResponse> {
    let response =
        crate::store::refresh_session(session_id.as_deref(), refresh_token.as_deref()).await?;
    catch_up_storage_after_write("refresh_token_for").await;
    Ok(response)
}

pub async fn verify_access_token(
    request: TokenVerifyRequest,
) -> AuthStackResult<TokenVerifyResponse> {
    if request.access_token.trim().is_empty() {
        return Err(AuthStackError::validation("access_token is required"));
    }
    crate::store::verify_access_token(&request).await
}

pub async fn logout_session(session_id: Option<String>) -> AuthStackResult<LogoutResponse> {
    let response = crate::store::revoke_session(session_id.as_deref()).await?;
    catch_up_storage_after_write("logout_session").await;
    Ok(response)
}

pub async fn get_jwks() -> AuthStackResult<JwksDocument> {
    crate::store::get_jwks().await
}

pub async fn list_signing_keys(
    admin_token: Option<String>,
) -> AuthStackResult<SigningKeyListResponse> {
    crate::store::validate_admin_token(admin_token.as_deref()).await?;
    crate::store::list_signing_keys().await
}

pub async fn rotate_signing_key(
    request: SigningKeyRotateRequest,
) -> AuthStackResult<SigningKeyRotateResponse> {
    crate::store::validate_admin_token(request.admin_token.as_deref()).await?;
    validate_signing_key_id(&request.kid)?;
    let response =
        crate::store::rotate_signing_key(&request.kid, request.retire_previous.unwrap_or(true))
            .await?;
    catch_up_storage_after_write("rotate_signing_key").await;
    Ok(response)
}

pub async fn storage_status(
    admin_token: Option<String>,
) -> AuthStackResult<StorageStatusResponse> {
    crate::store::validate_admin_token(admin_token.as_deref()).await?;
    crate::store::storage_status().await
}

pub async fn run_storage_projections(
    admin_token: Option<String>,
    batch_limit: Option<usize>,
) -> AuthStackResult<Vec<StorageProjectionRunResponse>> {
    crate::store::validate_admin_token(admin_token.as_deref()).await?;
    crate::store::catch_up_storage_projections(batch_limit).await
}

pub async fn check_authorization(
    request: AuthzCheckRequest,
) -> AuthStackResult<AuthzCheckResponse> {
    let tenant = TenantRef::new(request.tenant.as_str()).map_err(authz_validation_error)?;
    let subject = SubjectRef::new(request.subject.as_str()).map_err(authz_validation_error)?;
    let object = ObjectRef::new(request.object.as_str()).map_err(authz_validation_error)?;
    let relation = Relation::new(request.relation.as_str()).map_err(authz_validation_error)?;
    let model =
        load_authorization_model(&request.tenant, request.model_ref.as_ref(), object.type_name(), &relation).await?;
    let stored_tuples = crate::store::relationship_tuples_for_tenant(&request.tenant).await?;
    let context = AuthzContext {
        tenant_id: Some(tenant),
        attributes: request.context,
        ..AuthzContext::default()
    };
    let decision = Evaluator::new(model, stored_tuples)
        .check(&subject, &relation, &object, &context)
        .map_err(authz_validation_error)?;

    Ok(AuthzCheckResponse {
        allowed: decision.allowed,
        reason: decision
            .reason
            .unwrap_or_else(|| "direct relationship tuple matched".to_string()),
        model_id: decision.model_id,
    })
}

pub async fn list_authorized_objects(
    request: AuthzListObjectsRequest,
) -> AuthStackResult<AuthzListObjectsResponse> {
    let tenant = TenantRef::new(request.tenant.as_str()).map_err(authz_validation_error)?;
    let subject = SubjectRef::new(request.subject.as_str()).map_err(authz_validation_error)?;
    let relation = Relation::new(request.relation.as_str()).map_err(authz_validation_error)?;
    if request.object_type.trim().is_empty() {
        return Err(AuthStackError::validation("object_type is required"));
    }
    let model = load_authorization_model(
        &request.tenant,
        request.model_ref.as_ref(),
        &request.object_type,
        &relation,
    )
    .await?;
    let stored_tuples = crate::store::relationship_tuples_for_tenant(&request.tenant).await?;
    let context = AuthzContext {
        tenant_id: Some(tenant),
        attributes: request.context,
        ..AuthzContext::default()
    };
    let objects = Evaluator::new(model, stored_tuples)
        .list_objects(&subject, &relation, &request.object_type, &context)
        .map_err(authz_validation_error)?
        .into_iter()
        .map(|object| object.to_string())
        .collect();

    Ok(AuthzListObjectsResponse { objects })
}

pub async fn expand_authorization(
    request: AuthzExpandRequest,
) -> AuthStackResult<AuthzExpandResponse> {
    let tenant = TenantRef::new(request.tenant.as_str()).map_err(authz_validation_error)?;
    let object = ObjectRef::new(request.object.as_str()).map_err(authz_validation_error)?;
    let relation = Relation::new(request.relation.as_str()).map_err(authz_validation_error)?;
    let model =
        load_authorization_model(&request.tenant, request.model_ref.as_ref(), object.type_name(), &relation).await?;
    let stored_tuples = crate::store::relationship_tuples_for_tenant(&request.tenant).await?;
    let context = AuthzContext {
        tenant_id: Some(tenant),
        attributes: request.context,
        ..AuthzContext::default()
    };
    let graph = Evaluator::new(model, stored_tuples)
        .expand(&relation, &object, &context)
        .map_err(authz_validation_error)?;
    let graph_json =
        serde_json::to_string(&graph).map_err(|error| AuthStackError::serialization(error.to_string()))?;

    Ok(AuthzExpandResponse { graph_json })
}

pub async fn batch_check_authorization(
    request: AuthzBatchCheckRequest,
) -> AuthStackResult<AuthzBatchCheckResponse> {
    let mut results = Vec::with_capacity(request.checks.len());
    for check in request.checks {
        results.push(check_authorization(check).await?);
    }
    Ok(AuthzBatchCheckResponse { results })
}

pub async fn write_authorization_model(
    request: AuthzModelWriteRequest,
) -> AuthStackResult<AuthzModelWriteResponse> {
    if request.model_id.trim().is_empty() {
        return Err(AuthStackError::validation("model_id is required"));
    }
    if request.schema_json.trim().is_empty() {
        return Err(AuthStackError::validation("schema_json is required"));
    }
    validate_idempotency_key(request.idempotency_key.as_deref())?;
    let response = crate::store::write_authorization_model(&request).await?;
    catch_up_storage_after_write("write_authorization_model").await;
    Ok(response)
}

pub async fn activate_authorization_model(
    model_id: String,
    idempotency_key: Option<String>,
) -> AuthStackResult<AuthzModelWriteResponse> {
    if model_id.trim().is_empty() {
        return Err(AuthStackError::validation("model_id is required"));
    }
    validate_idempotency_key(idempotency_key.as_deref())?;
    let response = crate::store::activate_authorization_model(&model_id).await?;
    catch_up_storage_after_write("activate_authorization_model").await;
    Ok(response)
}

#[allow(dead_code)]
pub async fn read_authorization_model(
    model_id: String,
) -> AuthStackResult<AuthzModelReadResponse> {
    if model_id.trim().is_empty() {
        return Err(AuthStackError::validation("model_id is required"));
    }
    crate::store::read_authorization_model(&model_id).await
}

pub async fn write_relationship_tuples(
    request: RelationshipTupleWriteRequest,
) -> AuthStackResult<RelationshipTupleWriteResponse> {
    if request.tuples_json.trim().is_empty() {
        return Err(AuthStackError::validation("tuples_json is required"));
    }
    validate_idempotency_key(request.idempotency_key.as_deref())?;
    let response = crate::store::write_relationship_tuples(&request).await?;
    catch_up_storage_after_write("write_relationship_tuples").await;
    Ok(response)
}

pub async fn delete_relationship_tuples(
    request: RelationshipTupleWriteRequest,
) -> AuthStackResult<RelationshipTupleWriteResponse> {
    if request.tuples_json.trim().is_empty() {
        return Err(AuthStackError::validation("tuples_json is required"));
    }
    validate_idempotency_key(request.idempotency_key.as_deref())?;
    let response = crate::store::delete_relationship_tuples(&request).await?;
    catch_up_storage_after_write("delete_relationship_tuples").await;
    Ok(response)
}

#[allow(dead_code)]
pub async fn read_relationship_tuples(
    tenant: String,
    object: String,
    relation: String,
) -> AuthStackResult<String> {
    let _tenant = TenantRef::new(tenant.as_str()).map_err(authz_validation_error)?;
    let _object = ObjectRef::new(object.as_str()).map_err(authz_validation_error)?;
    let _relation = Relation::new(relation.as_str()).map_err(authz_validation_error)?;
    crate::store::read_relationship_tuples_json(&tenant, &object, &relation).await
}

pub async fn save_auth_provider_config(
    provider_id: String,
    enabled: bool,
) -> AuthStackResult<AuthProviderSummary> {
    validate_provider_id(&provider_id)?;
    let response = crate::store::save_auth_provider_config(&provider_id, enabled).await?;
    catch_up_storage_after_write("save_auth_provider_config").await;
    Ok(response)
}

pub async fn save_redirect_allowlist(redirects_json: String) -> AuthStackResult<bool> {
    let redirects: Vec<String> = serde_json::from_str(&redirects_json)
        .map_err(|error| AuthStackError::validation(format!("invalid redirects_json: {error}")))?;
    for redirect in redirects {
        if !redirect.starts_with('/') || redirect.starts_with("//") {
            return Err(AuthStackError::validation(
                "redirect allowlist entries must be local paths",
            ));
        }
    }
    crate::store::save_redirect_allowlist(&redirects_json).await?;
    catch_up_storage_after_write("save_redirect_allowlist").await;
    Ok(true)
}

async fn list_credentialed_auth_providers() -> AuthStackResult<Vec<AuthProviderSummary>> {
    let providers = crate::store::list_auth_providers().await?;
    let mut credentialed = Vec::new();
    for mut provider in providers {
        if provider_enabled(&provider.provider_id, provider.enabled).await
            && provider_has_credentials(&provider.provider_id).await
        {
            provider.enabled = true;
            credentialed.push(provider);
        }
    }
    Ok(credentialed)
}

async fn ensure_oauth_provider_ready(provider_id: &str) -> AuthStackResult<AuthProviderSummary> {
    let Some(mut provider) = crate::store::find_auth_provider(provider_id).await? else {
        return Err(AuthStackError::not_found(format!(
            "OAuth provider '{provider_id}' is not configured"
        )));
    };
    if !provider_enabled(provider_id, provider.enabled).await {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{provider_id}' is disabled"
        )));
    }
    if !provider_has_credentials(provider_id).await {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{provider_id}' is missing credentials"
        )));
    }
    provider.enabled = true;
    Ok(provider)
}

fn validate_email_password_login(request: &EmailPasswordLoginRequest) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    if request.password.is_empty() {
        return Err(AuthStackError::validation("password is required"));
    }
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_email_password_register(
    request: &EmailPasswordRegisterRequest,
    min_length: usize,
) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_password_policy(&request.password, min_length)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_password_reset_start(request: &PasswordResetStartRequest) -> AuthStackResult<()> {
    validate_required_email(&request.email)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_password_reset_complete(
    request: &PasswordResetCompleteRequest,
    min_length: usize,
) -> AuthStackResult<()> {
    if request.token.trim().is_empty() {
        return Err(AuthStackError::validation("reset token is required"));
    }
    validate_password_policy(&request.password, min_length)?;
    validate_safe_redirect_option(request.redirect_url.as_deref())?;
    Ok(())
}

fn validate_required_email(email: &str) -> AuthStackResult<()> {
    let email = email.trim();
    if email.is_empty() {
        return Err(AuthStackError::validation("email is required"));
    }
    if !email.contains('@') || !email.contains('.') {
        return Err(AuthStackError::validation(
            "email must be a valid email address",
        ));
    }
    Ok(())
}

fn validate_password_policy(password: &str, min_length: usize) -> AuthStackResult<()> {
    if password.len() < min_length {
        return Err(AuthStackError::validation(format!(
            "password must be at least {min_length} characters"
        )));
    }
    Ok(())
}

fn validate_passkey_verify_request(request: &PasskeyVerifyRequest) -> AuthStackResult<()> {
    if request.challenge_id.trim().is_empty() {
        return Err(AuthStackError::validation("challenge_id is required"));
    }
    if request.credential_json.trim().is_empty() {
        return Err(AuthStackError::validation("credential_json is required"));
    }
    Ok(())
}

fn validate_provider_id(provider_id: &str) -> AuthStackResult<()> {
    if provider_id.trim().is_empty() {
        return Err(AuthStackError::validation("provider_id is required"));
    }
    if !provider_id
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(AuthStackError::validation(
            "provider_id must contain only lowercase letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_signing_key_id(kid: &str) -> AuthStackResult<()> {
    let kid = kid.trim();
    if kid.is_empty() {
        return Err(AuthStackError::validation("kid is required"));
    }
    if kid.contains('/') || kid.contains('\\') || kid.chars().any(char::is_whitespace) {
        return Err(AuthStackError::validation("kid is invalid"));
    }
    Ok(())
}

fn validate_optional_email(email: &str) -> AuthStackResult<()> {
    let email = email.trim();
    if email.is_empty() || !email.contains('@') {
        return Err(AuthStackError::validation(
            "email must be empty or a valid email address",
        ));
    }
    Ok(())
}

fn validate_safe_redirect_option(value: Option<&str>) -> AuthStackResult<()> {
    if value.is_some_and(|value| !is_safe_redirect(value)) {
        return Err(AuthStackError::validation(
            "redirect_url must be a local path",
        ));
    }
    Ok(())
}

fn safe_redirect_or_default(redirect_url: Option<String>) -> String {
    redirect_url
        .filter(|value| is_safe_redirect(value))
        .unwrap_or_else(|| "/dashboard".to_string())
}

fn is_safe_redirect(value: &str) -> bool {
    value.starts_with('/') && !value.starts_with("//")
}

async fn password_min_length() -> usize {
    config_value("AUTH_PASSWORD_MIN_LENGTH")
        .await
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 8)
        .unwrap_or(DEFAULT_PASSWORD_MIN_LENGTH)
}

async fn feature_enabled(name: &str, default: bool) -> bool {
    config_value(name)
        .await
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

async fn provider_has_credentials(provider_id: &str) -> bool {
    match provider_id {
        "google" => {
            all_config_values_present(&["AUTH_GOOGLE_CLIENT_ID", "AUTH_GOOGLE_CLIENT_SECRET"]).await
        }
        "facebook" => {
            all_config_values_present(&["AUTH_FACEBOOK_CLIENT_ID", "AUTH_FACEBOOK_CLIENT_SECRET"])
                .await
        }
        "apple" => {
            all_config_values_present(&["AUTH_APPLE_CLIENT_ID"]).await
                && (all_config_values_present(&["AUTH_APPLE_GENERATED_CLIENT_SECRET"]).await
                    || all_config_values_present(&[
                        "AUTH_APPLE_TEAM_ID",
                        "AUTH_APPLE_KEY_ID",
                        "AUTH_APPLE_PRIVATE_KEY",
                    ])
                    .await)
        }
        other => {
            let upper = other.to_ascii_uppercase().replace(['-', '.'], "_");
            let client_id = format!("AUTH_{upper}_CLIENT_ID");
            let client_secret = format!("AUTH_{upper}_CLIENT_SECRET");
            all_config_values_present(&[client_id.as_str(), client_secret.as_str()]).await
        }
    }
}

async fn provider_enabled(provider_id: &str, stored_enabled: bool) -> bool {
    stored_enabled || feature_enabled(&provider_enabled_env_name(provider_id), false).await
}

fn provider_enabled_env_name(provider_id: &str) -> String {
    let upper = provider_id.to_ascii_uppercase().replace(['-', '.'], "_");
    format!("AUTH_{upper}_ENABLED")
}

async fn development_oauth_callback_bypass_enabled() -> bool {
    feature_enabled("AUTH_OAUTH_DEVELOPMENT_CALLBACK_BYPASS", false).await
}

async fn storage_auto_catch_up_enabled() -> bool {
    feature_enabled("AUTH_STORAGE_AUTO_CATCH_UP", true).await
}

async fn catch_up_storage_after_write(operation: &str) {
    if !storage_auto_catch_up_enabled().await {
        return;
    }
    match crate::store::catch_up_storage_projections(None).await {
        Ok(outcomes) => {
            tracing::debug!(
                operation,
                projection_count = outcomes.len(),
                "auth storage projections caught up after write"
            );
        }
        Err(error) => {
            tracing::error!(
                operation,
                error = %error,
                error_code = error.public_code(),
                "auth storage projection catch-up failed after write"
            );
        }
    }
}

async fn all_config_values_present(names: &[&str]) -> bool {
    for name in names {
        if !config_value(name)
            .await
            .is_some_and(|value| !value.trim().is_empty())
        {
            return false;
        }
    }
    true
}

async fn config_value(name: &str) -> Option<String> {
    #[cfg(all(feature = "sqlite", runtime_spin, not(test)))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await {
            return Some(value);
        }
    }

    std::env::var(name).ok()
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn development_oauth_callback_url(provider_id: &str, state: &str, redirect_url: &str) -> String {
    format!(
        "/api/auth/oauth/{provider_id}/callback?code=development-oauth-code&state={}&next={}",
        url_query_component(state),
        url_query_component(redirect_url)
    )
}

fn url_query_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

async fn load_authorization_model(
    tenant: &str,
    model_ref: Option<&AuthzModelRef>,
    object_type: &str,
    relation: &Relation,
) -> AuthStackResult<AuthorizationModel> {
    let model_ref = model_ref.ok_or_else(|| {
        AuthStackError::validation("model_ref is required; use {\"kind\":\"active\"} or {\"kind\":\"id\",\"model_id\":\"...\"}")
    })?;
    let model_id = match model_ref.kind.trim() {
        "active" => crate::store::active_authorization_model_id(tenant).await?,
        "id" => model_ref
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| AuthStackError::validation("model_ref.model_id is required when kind is 'id'"))?,
        _ => {
            return Err(AuthStackError::validation(
                "model_ref.kind must be 'active' or 'id'",
            ));
        }
    };
    if model_id == "bootstrap-deny-by-default" {
        return Ok(AuthorizationModel::new(model_id).with_type(
            ObjectType::new(object_type)
                .with_relation(relation.clone(), RelationDefinition::direct()),
        ));
    }
    let schema_json = crate::store::authorization_model_schema(tenant, &model_id).await?;
    AuthorizationModel::from_json(&schema_json).map_err(authz_validation_error)
}

fn validate_idempotency_key(value: Option<&str>) -> AuthStackResult<()> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(AuthStackError::validation("idempotency_key is required"));
    };
    if value.len() > 128 {
        return Err(AuthStackError::validation(
            "idempotency_key must be 128 characters or fewer",
        ));
    }
    Ok(())
}

fn authz_validation_error(error: ddd_authz::AuthzError) -> AuthStackError {
    AuthStackError::validation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_redirect_falls_back_to_dashboard() {
        assert_eq!(
            safe_redirect_or_default(Some("https://example.com".to_string())),
            "/dashboard"
        );
    }

    #[test]
    fn invalid_provider_id_is_rejected() {
        let error = validate_provider_id("../google").unwrap_err();

        assert_eq!(error.public_code(), "validation");
    }

    #[test]
    fn development_oauth_callback_url_encodes_redirect_component() {
        let url = development_oauth_callback_url("google", "state_1", "/dashboard?tab=home");

        assert_eq!(
            url,
            "/api/auth/oauth/google/callback?code=development-oauth-code&state=state_1&next=%2Fdashboard%3Ftab%3Dhome"
        );
    }

    #[test]
    fn session_cookie_header_value_adds_secure_when_enabled() {
        assert_eq!(
            session_cookie_header_value("session_1", Some(3600), true),
            "ddd_auth_session=session_1; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600; Secure"
        );
    }

    #[test]
    fn session_cookie_header_value_omits_secure_when_disabled() {
        assert_eq!(
            session_cookie_header_value("session_1", None, false),
            "ddd_auth_session=session_1; Path=/; HttpOnly; SameSite=Lax"
        );
    }
}
