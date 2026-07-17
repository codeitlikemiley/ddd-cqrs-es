#![allow(unused_imports)]

use super::common::*;
use crate::contracts::*;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[server(prefix = "/api/ui")]
pub async fn get_auth_capabilities() -> Result<AuthCapabilities, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::auth_capabilities()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn register_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::register_email_password(EmailPasswordRegisterRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_email_verification(
    token: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response =
            crate::application::complete_email_verification(EmailVerificationCompleteRequest {
                token,
                redirect_url,
            })
            .await
            .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn resend_email_verification(
    email: String,
    redirect_url: Option<String>,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::resend_email_verification(EmailVerificationResendRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn development_mail_capture_enabled() -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        Ok(crate::auth_product::development_mail_capture_enabled().await)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn latest_development_mail(
    recipient: String,
    message_kind: String,
) -> Result<CapturedMailResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::latest_captured_mail(recipient, message_kind)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (recipient, message_kind);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn login_email_password(
    email: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::login_email_password(EmailPasswordLoginRequest {
            email,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_password_reset(
    email: String,
    redirect_url: Option<String>,
) -> Result<PasswordResetStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_password_reset(PasswordResetStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_password_reset(
    token: String,
    password: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_password_reset(PasswordResetCompleteRequest {
            token,
            password,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (token, password, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_auth_providers() -> Result<Vec<AuthProviderSummary>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_auth_providers()
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn get_current_session() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_current_session_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authenticated_route() -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authenticated_route_for(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn require_authorized_route(permission: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::require_authorized_route_for(
            &permission,
            current_session_id_from_cookie(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = permission;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_registration(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_registration(
            PasskeyStartRequest {
                email,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_registration(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_registration(
            PasskeyVerifyRequest {
                challenge_id,
                credential_json,
                redirect_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_passkey_login(
    email: Option<String>,
    redirect_url: Option<String>,
) -> Result<PasskeyStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_passkey_login(PasskeyStartRequest {
            email,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (email, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_passkey_login(
    challenge_id: String,
    credential_json: String,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::verify_passkey_login(PasskeyVerifyRequest {
            challenge_id,
            credential_json,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (challenge_id, credential_json, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn start_oauth_login(
    provider_id: String,
    redirect_url: Option<String>,
) -> Result<OAuthStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_oauth_login(provider_id, redirect_url)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn complete_oauth_callback(
    provider_id: String,
    code: Option<String>,
    state: Option<String>,
    redirect_url: Option<String>,
) -> Result<LoginCompletionResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::complete_oauth_callback(OAuthCallbackRequest {
            provider_id,
            code,
            state,
            redirect_url,
        })
        .await
        .map_err(server_fn_error)?;
        set_session_cookie(&response).await;
        Ok(browser_login_response(response))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (provider_id, code, state, redirect_url);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn logout_current_session() -> Result<LogoutResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let response = crate::application::logout_session(current_session_id_from_cookie())
            .await
            .map_err(server_fn_error)?;
        clear_session_cookie().await;
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}
