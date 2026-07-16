//! Application services shared by Leptos server functions, REST, and gRPC.
//!
//! Domain modules re-exported so existing `crate::application::*` call sites keep working.

mod account;
mod admin;
mod auth;
mod authorization;
mod common;
mod dashboard;
mod ingress;
mod organization;
mod profile;
mod request_auth;
mod session;
mod vault;

// pub(crate) re-exports include pub(crate) helpers so sibling modules and the
// rest of the crate can use crate::application::name after the split.
pub(crate) use account::*;
pub(crate) use admin::*;
pub(crate) use auth::*;
pub(crate) use authorization::*;
pub(crate) use common::*;
pub(crate) use dashboard::*;
pub(crate) use ingress::*;
pub(crate) use organization::*;
pub(crate) use profile::*;
pub(crate) use request_auth::*;
pub(crate) use session::*;
pub(crate) use vault::*;

#[cfg(test)]
mod tests {
    use super::*;
    use wasi_auth::cedar::{
        CedarProvider, DEFAULT_APPLICATION_POLICY, DEFAULT_APPLICATION_POLICY_REVISION,
    };

    #[test]
    fn unsafe_redirect_falls_back_to_dashboard() {
        assert_eq!(
            safe_redirect_or_default(Some("https://example.com".to_string())),
            "/dashboard"
        );
    }

    #[test]
    fn browser_origin_accepts_loopback_aliases() {
        assert!(browser_origins_match(
            "http://localhost:3008",
            "http://127.0.0.1:3008"
        ));
        assert!(browser_origins_match(
            "http://127.0.0.1:3008",
            "http://localhost:3008"
        ));
        assert!(browser_origins_match(
            "http://[::1]:3008",
            "http://localhost:3008"
        ));
        assert!(browser_origins_match(
            "http://localhost:3008",
            "http://localhost:3008/"
        ));
    }

    #[test]
    fn browser_origin_rejects_host_or_port_mismatch() {
        assert!(!browser_origins_match(
            "http://localhost:3009",
            "http://localhost:3008"
        ));
        assert!(!browser_origins_match(
            "https://localhost:3008",
            "http://localhost:3008"
        ));
        assert!(!browser_origins_match(
            "http://evil.example:3008",
            "http://localhost:3008"
        ));
        assert!(!browser_origins_match(
            "http://localhost:3008",
            "http://example.com:3008"
        ));
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
            "__Host-session=session_1; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600; Secure"
        );
    }

    #[test]
    fn session_cookie_header_value_omits_secure_when_disabled() {
        assert_eq!(
            session_cookie_header_value("session_1", None, false),
            "wasi_auth_dev_session=session_1; Path=/; HttpOnly; SameSite=Lax"
        );
    }

    #[test]
    fn embedded_cedar_policy_passes_strict_validation() {
        assert!(
            CedarProvider::new_validated(
                DEFAULT_APPLICATION_POLICY,
                wasi_auth::cedar::DEFAULT_APPLICATION_SCHEMA,
                "[]",
                DEFAULT_APPLICATION_POLICY_REVISION,
            )
            .is_ok()
        );
        assert!(embedded_cedar_provider().is_ok());
    }
}
