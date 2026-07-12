use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use wasi_auth::authentication::WorkflowError;
use wasi_auth::authentication::jwt::{
    Algorithm, DecodingKey, EncodingKey, Header, IdTokenClaims, JwksDocument, JwksKey,
    decode_id_token, encode_jwt, jwks_key_by_id, jwt_key_id,
};

use crate::error::{AuthStackError, AuthStackResult};
use crate::store::ConsumedOauthGrant;

#[derive(Clone, Debug)]
pub struct VerifiedOAuthIdentity {
    pub provider_id: String,
    pub provider_subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
struct OAuthProviderRuntimeConfig {
    provider_id: String,
    issuer: String,
    authorization_url: String,
    token_url: String,
    userinfo_url: Option<String>,
    jwks_url: Option<String>,
    jwks_json: Option<String>,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct OAuthTokenResponse {
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct OAuthUserinfoResponse {
    #[serde(alias = "id", alias = "sub")]
    subject: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct AppleClientSecretClaims {
    iss: String,
    iat: u64,
    exp: u64,
    aud: String,
    sub: String,
}

const APPLE_CLIENT_SECRET_AUDIENCE: &str = "https://appleid.apple.com";
const DEFAULT_APPLE_CLIENT_SECRET_TTL_SECONDS: u64 = 86_400;
const MAX_APPLE_CLIENT_SECRET_TTL_SECONDS: u64 = 15_777_000;

pub async fn authorization_url(
    provider_id: &str,
    state: &str,
    nonce: &str,
    pkce_challenge: &str,
) -> AuthStackResult<String> {
    let config = oauth_provider_runtime_config(provider_id).await?;
    let scope = config.scopes.join(" ");
    Ok(format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
        config.authorization_url,
        url_query_component(&config.client_id),
        url_query_component(&config.redirect_uri),
        url_query_component(&scope),
        url_query_component(state),
        url_query_component(nonce),
        url_query_component(pkce_challenge),
    ))
}

pub async fn complete_authorization_code(
    provider_id: &str,
    code: &str,
    grant: &ConsumedOauthGrant,
) -> AuthStackResult<VerifiedOAuthIdentity> {
    let config = oauth_provider_runtime_config(provider_id).await?;
    let token_response = exchange_authorization_code(&config, code, &grant.pkce_verifier).await?;
    if let Some(id_token) = token_response
        .id_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let jwks = provider_jwks(&config).await?;
        let claims = validate_provider_id_token(&config, id_token, &jwks, &grant.nonce)?;

        return Ok(VerifiedOAuthIdentity {
            provider_id: provider_id.to_string(),
            provider_subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified,
            name: claims.name,
        });
    }

    let profile = fetch_provider_userinfo(&config, &token_response).await?;

    Ok(VerifiedOAuthIdentity {
        provider_id: provider_id.to_string(),
        provider_subject: profile.subject,
        email: profile.email,
        email_verified: profile.email_verified,
        name: profile.name,
    })
}

fn validate_provider_id_token(
    config: &OAuthProviderRuntimeConfig,
    id_token: &str,
    jwks: &JwksDocument,
    expected_nonce: &str,
) -> AuthStackResult<IdTokenClaims> {
    let key_id = jwt_key_id(id_token).map_err(map_auth_error)?;
    let key = jwks_key_by_id(jwks, &key_id).map_err(map_auth_error)?;
    let algorithm = id_token_algorithm(key)?;
    let decoding_key = decoding_key_from_jwks_key(key, algorithm)?;
    decode_id_token(
        id_token,
        &decoding_key,
        &config.issuer,
        &config.client_id,
        &[algorithm],
        Some(expected_nonce),
    )
    .map_err(map_auth_error)
}

async fn exchange_authorization_code(
    config: &OAuthProviderRuntimeConfig,
    code: &str,
    pkce_verifier: &str,
) -> AuthStackResult<OAuthTokenResponse> {
    let body = form_urlencoded(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &config.redirect_uri),
        ("client_id", &config.client_id),
        ("client_secret", &config.client_secret),
        ("code_verifier", pkce_verifier),
    ]);
    let response = outbound_post_form(&config.token_url, body).await?;
    serde_json::from_slice(&response).map_err(|error| {
        AuthStackError::transport(format!("OAuth token response is invalid JSON: {error}"))
    })
}

async fn provider_jwks(config: &OAuthProviderRuntimeConfig) -> AuthStackResult<JwksDocument> {
    if let Some(value) = config
        .jwks_json
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return parse_jwks_document(value);
    }
    let Some(url) = config
        .jwks_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{}' is missing JWKS configuration",
            config.provider_id
        )));
    };
    let response = outbound_get(url).await?;
    serde_json::from_slice(&response).map_err(|error| {
        AuthStackError::configuration(format!("OAuth JWKS response is invalid JSON: {error}"))
    })
}

async fn oauth_provider_runtime_config(
    provider_id: &str,
) -> AuthStackResult<OAuthProviderRuntimeConfig> {
    let prefix = provider_env_prefix(provider_id);
    let client_id = required_config_value(&format!("AUTH_{prefix}_CLIENT_ID")).await?;
    let client_secret = oauth_client_secret(provider_id, &prefix).await?;
    let issuer = config_value(&format!("AUTH_{prefix}_ISSUER"))
        .await
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_issuer(provider_id).map(ToOwned::to_owned))
        .ok_or_else(|| {
            AuthStackError::configuration(format!(
                "OAuth provider '{provider_id}' is missing issuer configuration"
            ))
        })?;
    let authorization_url = config_value(&format!("AUTH_{prefix}_AUTHORIZATION_URL"))
        .await
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_authorization_url(provider_id).map(ToOwned::to_owned))
        .ok_or_else(|| {
            AuthStackError::configuration(format!(
                "OAuth provider '{provider_id}' is missing authorization endpoint configuration"
            ))
        })?;
    let token_url = config_value(&format!("AUTH_{prefix}_TOKEN_URL"))
        .await
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_token_url(provider_id).map(ToOwned::to_owned))
        .ok_or_else(|| {
            AuthStackError::configuration(format!(
                "OAuth provider '{provider_id}' is missing token endpoint configuration"
            ))
        })?;
    let userinfo_url = config_value(&format!("AUTH_{prefix}_USERINFO_URL"))
        .await
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_userinfo_url(provider_id).map(ToOwned::to_owned));
    let jwks_url = config_value(&format!("AUTH_{prefix}_JWKS_URL"))
        .await
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_jwks_url(provider_id).map(ToOwned::to_owned));
    let jwks_json = config_value(&format!("AUTH_{prefix}_JWKS_JSON"))
        .await
        .filter(|value| !value.trim().is_empty());
    let scopes = oauth_scopes(provider_id, &prefix).await;
    let redirect_uri = oauth_redirect_uri(provider_id, &prefix).await;

    Ok(OAuthProviderRuntimeConfig {
        provider_id: provider_id.to_string(),
        issuer,
        authorization_url,
        token_url,
        userinfo_url,
        jwks_url,
        jwks_json,
        client_id,
        client_secret,
        redirect_uri,
        scopes,
    })
}

async fn oauth_client_secret(provider_id: &str, prefix: &str) -> AuthStackResult<String> {
    if let Some(value) = config_value(&format!("AUTH_{prefix}_CLIENT_SECRET"))
        .await
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(value);
    }
    if provider_id == "apple" {
        if let Some(value) = config_value("AUTH_APPLE_GENERATED_CLIENT_SECRET")
            .await
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(value);
        }
        return generate_apple_client_secret().await;
    }
    Err(AuthStackError::configuration(format!(
        "OAuth provider '{provider_id}' is missing client secret configuration"
    )))
}

async fn generate_apple_client_secret() -> AuthStackResult<String> {
    let client_id = required_config_value("AUTH_APPLE_CLIENT_ID").await?;
    let team_id = required_config_value("AUTH_APPLE_TEAM_ID").await?;
    let key_id = required_config_value("AUTH_APPLE_KEY_ID").await?;
    let private_key = required_config_value("AUTH_APPLE_PRIVATE_KEY").await?;
    let ttl_seconds = config_u64(
        "AUTH_APPLE_CLIENT_SECRET_TTL_SECONDS",
        DEFAULT_APPLE_CLIENT_SECRET_TTL_SECONDS,
    )
    .await;
    let ttl_seconds = apple_client_secret_ttl_seconds(ttl_seconds);
    let now = now_seconds();
    let claims = AppleClientSecretClaims {
        iss: team_id,
        iat: now,
        exp: now.saturating_add(ttl_seconds),
        aud: APPLE_CLIENT_SECRET_AUDIENCE.to_string(),
        sub: client_id,
    };
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id);
    let normalized_key = normalize_pem_value(&private_key);
    let encoding_key = EncodingKey::from_ec_pem(normalized_key.as_bytes()).map_err(|error| {
        AuthStackError::configuration(format!("AUTH_APPLE_PRIVATE_KEY is invalid: {error}"))
    })?;

    encode_jwt(&header, &claims, &encoding_key).map_err(|error| {
        AuthStackError::configuration(format!("failed to generate Apple client secret: {error}"))
    })
}

async fn oauth_scopes(provider_id: &str, prefix: &str) -> Vec<String> {
    config_value(&format!("AUTH_{prefix}_SCOPES"))
        .await
        .filter(|value| !value.trim().is_empty())
        .map(|value| split_scopes(&value))
        .unwrap_or_else(|| default_scopes(provider_id))
}

async fn oauth_redirect_uri(provider_id: &str, prefix: &str) -> String {
    if let Some(value) = config_value(&format!("AUTH_{prefix}_REDIRECT_URI"))
        .await
        .filter(|value| !value.trim().is_empty())
    {
        return value;
    }
    let base_url = config_value("AUTH_PUBLIC_BASE_URL")
        .await
        .or_else(|| std::env::var("AUTH_JWT_ISSUER").ok())
        .unwrap_or_else(|| "http://127.0.0.1:3008".to_string());
    format!(
        "{}/api/auth/oauth/{provider_id}/callback",
        base_url.trim_end_matches('/')
    )
}

fn split_scopes(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch == ',' || ch.is_ascii_whitespace())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn default_scopes(provider_id: &str) -> Vec<String> {
    match provider_id {
        "apple" => vec!["openid", "email", "name"],
        "facebook" => vec!["email", "public_profile"],
        _ => vec!["openid", "email", "profile"],
    }
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn default_issuer(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google" => Some("https://accounts.google.com"),
        "apple" => Some("https://appleid.apple.com"),
        "facebook" => Some("https://www.facebook.com"),
        _ => None,
    }
}

fn default_authorization_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google" => Some("https://accounts.google.com/o/oauth2/v2/auth"),
        "apple" => Some("https://appleid.apple.com/auth/authorize"),
        "facebook" => Some("https://www.facebook.com/v20.0/dialog/oauth"),
        _ => None,
    }
}

fn default_token_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google" => Some("https://oauth2.googleapis.com/token"),
        "apple" => Some("https://appleid.apple.com/auth/token"),
        "facebook" => Some("https://graph.facebook.com/v20.0/oauth/access_token"),
        _ => None,
    }
}

fn default_jwks_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "google" => Some("https://www.googleapis.com/oauth2/v3/certs"),
        "apple" => Some("https://appleid.apple.com/auth/keys"),
        _ => None,
    }
}

fn default_userinfo_url(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "facebook" => Some("https://graph.facebook.com/v20.0/me?fields=id,email,name"),
        "google" => Some("https://openidconnect.googleapis.com/v1/userinfo"),
        _ => None,
    }
}

async fn fetch_provider_userinfo(
    config: &OAuthProviderRuntimeConfig,
    token_response: &OAuthTokenResponse,
) -> AuthStackResult<OAuthUserinfoResponse> {
    let Some(userinfo_url) = config
        .userinfo_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{}' did not return an id_token and is missing userinfo configuration",
            config.provider_id
        )));
    };
    let access_token = token_response
        .access_token
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthStackError::configuration(format!(
                "OAuth provider '{}' did not return an access_token for userinfo lookup",
                config.provider_id
            ))
        })?;
    if let Some(token_type) = token_response.token_type.as_deref()
        && !token_type.eq_ignore_ascii_case("bearer")
    {
        return Err(AuthStackError::configuration(format!(
            "OAuth provider '{}' returned unsupported token_type '{token_type}'",
            config.provider_id
        )));
    }
    let response = outbound_get_bearer(userinfo_url, access_token).await?;
    serde_json::from_slice(&response).map_err(|error| {
        AuthStackError::transport(format!("OAuth userinfo response is invalid JSON: {error}"))
    })
}

fn provider_env_prefix(provider_id: &str) -> String {
    provider_id.to_ascii_uppercase().replace(['-', '.'], "_")
}

async fn required_config_value(name: &str) -> AuthStackResult<String> {
    config_value(name)
        .await
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthStackError::configuration(format!("{name} is required")))
}

async fn config_value(name: &str) -> Option<String> {
    #[cfg(all(any(feature = "sqlite", feature = "postgres"), runtime_spin))]
    {
        let variable_name = name.to_ascii_lowercase();
        if let Ok(value) = spin_sdk::variables::get(&variable_name).await {
            return Some(value);
        }
    }

    std::env::var(name).ok()
}

async fn config_u64(name: &str, default: u64) -> u64 {
    config_value(name)
        .await
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn apple_client_secret_ttl_seconds(configured: u64) -> u64 {
    configured.min(MAX_APPLE_CLIENT_SECRET_TTL_SECONDS)
}

fn normalize_pem_value(value: &str) -> String {
    value.trim().replace("\\n", "\n")
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn id_token_algorithm(key: &JwksKey) -> AuthStackResult<Algorithm> {
    match (key.kty.as_str(), key.alg.as_str()) {
        ("RSA", "" | "RS256") => Ok(Algorithm::RS256),
        ("EC", "" | "ES256") => Ok(Algorithm::ES256),
        (_, "RS256") => Ok(Algorithm::RS256),
        (_, "ES256") => Ok(Algorithm::ES256),
        _ => Err(AuthStackError::InvalidToken),
    }
}

fn decoding_key_from_jwks_key(key: &JwksKey, algorithm: Algorithm) -> AuthStackResult<DecodingKey> {
    match algorithm {
        Algorithm::RS256 => {
            let modulus = key
                .public_parameters
                .get("n")
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            let exponent = key
                .public_parameters
                .get("e")
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            DecodingKey::from_rsa_components(modulus, exponent)
                .map_err(|_| AuthStackError::InvalidToken)
        }
        Algorithm::ES256 => {
            let x = key
                .public_parameters
                .get("x")
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            let y = key
                .public_parameters
                .get("y")
                .filter(|value| !value.trim().is_empty())
                .ok_or(AuthStackError::InvalidToken)?;
            DecodingKey::from_ec_components(x, y).map_err(|_| AuthStackError::InvalidToken)
        }
        _ => Err(AuthStackError::InvalidToken),
    }
}

fn parse_jwks_document(value: &str) -> AuthStackResult<JwksDocument> {
    let parsed: Value = serde_json::from_str(value).map_err(|error| {
        AuthStackError::configuration(format!("OAuth JWKS JSON is invalid: {error}"))
    })?;
    if parsed.get("keys").is_some() {
        serde_json::from_value(parsed).map_err(|error| {
            AuthStackError::configuration(format!("OAuth JWKS document is invalid: {error}"))
        })
    } else {
        let key = jwks_key_from_json_value(parsed)?;
        Ok(JwksDocument { keys: vec![key] })
    }
}

fn jwks_key_from_json_value(value: Value) -> AuthStackResult<JwksKey> {
    serde_json::from_value(value)
        .map_err(|error| AuthStackError::configuration(format!("OAuth JWK is invalid: {error}")))
}

fn form_urlencoded(parts: &[(&str, &str)]) -> String {
    parts
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                url_query_component(key),
                url_query_component(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn url_query_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            b' ' => vec!['+'],
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn map_auth_error(error: WorkflowError) -> AuthStackError {
    match error {
        WorkflowError::SessionExpired => AuthStackError::SessionExpired,
        WorkflowError::Validation { message } => AuthStackError::validation(message),
        _ => AuthStackError::InvalidToken,
    }
}

async fn outbound_get(url: &str) -> AuthStackResult<Vec<u8>> {
    #[cfg(all(any(feature = "sqlite", feature = "postgres"), runtime_spin))]
    {
        use http_body_util::BodyExt;

        let response = spin_sdk::http::get(url).await.map_err(|error| {
            AuthStackError::transport(format!("OAuth GET request failed: {error}"))
        })?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| {
                AuthStackError::transport(format!("OAuth GET response body failed: {error:?}"))
            })?
            .to_bytes()
            .to_vec();
        if !status.is_success() {
            return Err(AuthStackError::transport(format!(
                "OAuth GET request returned status {status}"
            )));
        }
        Ok(bytes)
    }

    #[cfg(not(all(any(feature = "sqlite", feature = "postgres"), runtime_spin)))]
    {
        let _ = url;
        Err(AuthStackError::configuration(
            "OAuth provider HTTP requests require Spin runtime",
        ))
    }
}

async fn outbound_get_bearer(url: &str, bearer_token: &str) -> AuthStackResult<Vec<u8>> {
    #[cfg(all(any(feature = "sqlite", feature = "postgres"), runtime_spin))]
    {
        use http_body_util::BodyExt;
        use spin_sdk::http::{FullBody, send};

        let request = http::Request::get(url)
            .header(http::header::ACCEPT, "application/json")
            .header(
                http::header::AUTHORIZATION,
                format!("Bearer {bearer_token}"),
            )
            .body(FullBody::new(bytes::Bytes::new()))
            .map_err(|error| {
                AuthStackError::transport(format!("OAuth userinfo request build failed: {error}"))
            })?;
        let response = send(request).await.map_err(|error| {
            AuthStackError::transport(format!("OAuth userinfo request failed: {error}"))
        })?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| {
                AuthStackError::transport(format!("OAuth userinfo response body failed: {error:?}"))
            })?
            .to_bytes()
            .to_vec();
        if !status.is_success() {
            return Err(AuthStackError::transport(format!(
                "OAuth userinfo endpoint returned status {status}"
            )));
        }
        Ok(bytes)
    }

    #[cfg(not(all(any(feature = "sqlite", feature = "postgres"), runtime_spin)))]
    {
        let _ = (url, bearer_token);
        Err(AuthStackError::configuration(
            "OAuth provider HTTP requests require Spin runtime",
        ))
    }
}

async fn outbound_post_form(url: &str, body: String) -> AuthStackResult<Vec<u8>> {
    #[cfg(all(any(feature = "sqlite", feature = "postgres"), runtime_spin))]
    {
        use bytes::Bytes;
        use http_body_util::BodyExt;
        use spin_sdk::http::{FullBody, send};

        let request = http::Request::post(url)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(http::header::ACCEPT, "application/json")
            .body(FullBody::new(Bytes::from(body)))
            .map_err(|error| {
                AuthStackError::transport(format!("OAuth token request build failed: {error}"))
            })?;
        let response = send(request).await.map_err(|error| {
            AuthStackError::transport(format!("OAuth token request failed: {error}"))
        })?;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| {
                AuthStackError::transport(format!("OAuth token response body failed: {error:?}"))
            })?
            .to_bytes()
            .to_vec();
        if !status.is_success() {
            return Err(AuthStackError::transport(format!(
                "OAuth token endpoint returned status {status}"
            )));
        }
        Ok(bytes)
    }

    #[cfg(not(all(any(feature = "sqlite", feature = "postgres"), runtime_spin)))]
    {
        let _ = (url, body);
        Err(AuthStackError::configuration(
            "OAuth provider HTTP requests require Spin runtime",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_urlencoded_encodes_spaces_as_plus_and_reserved_bytes() {
        let body = form_urlencoded(&[("scope", "openid email"), ("redirect_uri", "/oauth/cb")]);

        assert_eq!(body, "scope=openid+email&redirect_uri=%2Foauth%2Fcb");
    }

    #[test]
    fn default_google_authorization_url_is_oidc_endpoint() {
        assert_eq!(
            default_authorization_url("google"),
            Some("https://accounts.google.com/o/oauth2/v2/auth")
        );
    }

    #[test]
    fn parse_jwks_document_accepts_single_jwk() {
        let jwks = parse_jwks_document(
            r#"{"kid":"kid-1","kty":"RSA","alg":"RS256","use":"sig","n":"abc","e":"AQAB"}"#,
        )
        .unwrap();

        assert_eq!(jwks.keys[0].kid, "kid-1");
    }

    #[test]
    fn id_token_algorithm_rejects_unsupported_key_type() {
        use std::collections::BTreeMap;

        let key = JwksKey {
            kid: "kid-1".to_string(),
            kty: "oct".to_string(),
            alg: "HS256".to_string(),
            use_: "sig".to_string(),
            public_parameters: BTreeMap::new(),
        };

        assert_eq!(
            id_token_algorithm(&key).unwrap_err().public_code(),
            "invalid_token"
        );
    }

    #[test]
    fn default_facebook_userinfo_url_requests_profile_fields() {
        assert_eq!(
            default_userinfo_url("facebook"),
            Some("https://graph.facebook.com/v20.0/me?fields=id,email,name")
        );
    }

    #[test]
    fn facebook_userinfo_deserializes_id_alias() {
        let profile: OAuthUserinfoResponse = serde_json::from_str(
            r#"{"id":"facebook-123","email":"user@example.test","name":"Example User"}"#,
        )
        .unwrap();

        assert_eq!(profile.subject, "facebook-123");
        assert_eq!(profile.email.as_deref(), Some("user@example.test"));
        assert_eq!(profile.name.as_deref(), Some("Example User"));
    }

    #[test]
    fn oidc_userinfo_deserializes_sub_alias() {
        let profile: OAuthUserinfoResponse =
            serde_json::from_str(r#"{"sub":"subject-123","email_verified":true}"#).unwrap();

        assert_eq!(profile.subject, "subject-123");
        assert_eq!(profile.email_verified, Some(true));
    }

    #[test]
    fn apple_client_secret_ttl_is_clamped_to_provider_limit() {
        assert_eq!(
            apple_client_secret_ttl_seconds(MAX_APPLE_CLIENT_SECRET_TTL_SECONDS + 1),
            MAX_APPLE_CLIENT_SECRET_TTL_SECONDS
        );
        assert_eq!(apple_client_secret_ttl_seconds(3600), 3600);
    }

    #[test]
    fn normalize_pem_value_expands_escaped_newlines() {
        assert_eq!(
            normalize_pem_value("-----BEGIN PRIVATE KEY-----\\nabc\\n-----END PRIVATE KEY-----"),
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"
        );
    }
}
