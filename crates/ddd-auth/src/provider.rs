use crate::AuthProviderId;

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthProviderConfig {
    pub provider_id: AuthProviderId,
    pub profile: OAuthProviderProfile,
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub client_id_env: String,
    pub client_secret_ref: String,
    pub scopes: Vec<String>,
    pub redirect_uri_allowlist: Vec<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OAuthProviderProfile {
    Google,
    Apple,
    Facebook,
    Custom,
}
