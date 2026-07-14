#![allow(unused_imports)]

use super::common::*;
use crate::contracts::*;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[server(prefix = "/api/ui")]
pub async fn save_auth_provider(
    provider_id: String,
    enabled: bool,
) -> Result<AuthProviderSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::admin_save_provider(provider_id, enabled, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, enabled);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn save_redirect_allowlist(redirects_json: String) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::save_redirect_allowlist(redirects_json, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = redirects_json;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_signing_keys() -> Result<SigningKeyListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_signing_keys(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn rotate_signing_key(
    kid: String,
    retire_previous: bool,
) -> Result<SigningKeyRotateResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::rotate_signing_key(
            SigningKeyRotateRequest {
                kid,
                retire_previous: Some(retire_previous),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (kid, retire_previous);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_authorization_capabilities()
-> Result<AuthorizationCapabilitiesResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::authorization_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_admin_users() -> Result<AdminUserListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_admin_users(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_admin_health() -> Result<HealthStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_health(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn list_policy_versions() -> Result<PolicyVersionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_policy_versions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn publish_policy_version(
    policy_text: String,
    schema_text: String,
) -> Result<crate::contracts::PolicyVersionSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::publish_policy(
            PolicyPublishRequest {
                policy_text,
                schema_text,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (policy_text, schema_text);
        unreachable!()
    }
}
