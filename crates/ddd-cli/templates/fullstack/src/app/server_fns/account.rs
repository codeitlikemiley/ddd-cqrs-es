#![allow(unused_imports)]

use super::common::*;
use crate::contracts::*;
use leptos::prelude::*;
use server_fn::ServerFnError;
use server_fn::codec::Json;

#[server(prefix = "/api/ui")]
pub async fn get_account_profile() -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_account_profile(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn update_account_profile(
    first_name: String,
    last_name: String,
    display_name: String,
    username: String,
    is_public: bool,
    avatar_data_url: Option<String>,
) -> Result<ProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::update_account_profile(
            ProfileUpdateRequest {
                first_name,
                last_name,
                display_name,
                username,
                is_public,
                avatar_data_url,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            first_name,
            last_name,
            display_name,
            username,
            is_public,
            avatar_data_url,
        );
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_public_profile(username: String) -> Result<PublicProfileView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::get_public_profile(username)
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = username;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::change_password(
            PasswordChangeRequest {
                current_password,
                new_password,
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (current_password, new_password);
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn list_account_sessions() -> Result<AccountSessionListResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::list_sessions(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn revoke_account_session(session_id: String) -> Result<AcceptedResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let current_session = current_session_id_from_cookie();
        let response = crate::application::revoke_account_session(
            SessionRevokeRequest {
                session_id: session_id.clone(),
            },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)?;
        if current_session.as_deref() == Some(session_id.as_str()) {
            clear_session_cookie().await;
        }
        Ok(response)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = session_id;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn get_mfa_status() -> Result<MfaStatusResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::mfa_status(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn start_totp_enrollment() -> Result<MfaEnrollStartResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::start_totp_enrollment(server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    unreachable!()
}

#[server(prefix = "/api/ui")]
pub async fn confirm_totp_enrollment(
    code: String,
) -> Result<MfaEnrollConfirmResponse, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::confirm_totp_enrollment(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_totp_step_up(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::verify_totp_step_up(MfaCodeRequest { code }, server_fn_request_auth())
            .await
            .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}

#[server(prefix = "/api/ui")]
pub async fn verify_recovery_code(code: String) -> Result<SessionView, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        crate::application::use_recovery_code_for_step_up(
            MfaCodeRequest { code },
            server_fn_request_auth(),
        )
        .await
        .map_err(server_fn_error)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = code;
        unreachable!()
    }
}
