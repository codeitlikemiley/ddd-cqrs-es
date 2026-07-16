//! Leptos server functions (`/api/ui/*`) — thin adapters over application services.

mod account;
mod admin;
mod auth;
mod common;
mod dashboard;
mod organizations;

pub use account::*;
pub use admin::*;
pub use auth::*;
pub use common::*;
pub use dashboard::*;
pub use organizations::*;

#[cfg(test)]
mod tests {
    use super::common::browser_login_response;
    use crate::contracts::LoginCompletionResponse;

    #[test]
    fn browser_login_response_removes_browser_visible_tokens() {
        let response = LoginCompletionResponse {
            authenticated: true,
            redirect_url: "/dashboard".to_string(),
            session_id: Some("session_123".to_string()),
            access_token: Some("access-token".to_string()),
            refresh_token: Some("refresh-token".to_string()),
            expires_in_seconds: 3600,
        };

        let redacted = browser_login_response(response);

        assert!(redacted.authenticated);
        assert_eq!(redacted.redirect_url, "/dashboard");
        assert_eq!(redacted.expires_in_seconds, 3600);
        assert_eq!(redacted.session_id, None);
        assert_eq!(redacted.access_token, None);
        assert_eq!(redacted.refresh_token, None);
    }
}
