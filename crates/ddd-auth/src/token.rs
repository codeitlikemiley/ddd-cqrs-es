use crate::{AuthError, SessionId, TenantId, UserId};
use std::collections::BTreeMap;

#[cfg(feature = "jwt")]
use jsonwebtoken::{
    decode, decode_header, encode, errors::ErrorKind as JwtErrorKind, Algorithm, DecodingKey,
    EncodingKey, Header, Validation,
};

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: Vec<String>,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
    pub tenant_id: Option<TenantId>,
    pub session_id: Option<SessionId>,
    pub roles: Vec<String>,
    pub scope: Vec<String>,
    pub auth_time: Option<u64>,
    pub extra: BTreeMap<String, String>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: Vec<String>,
    pub exp: u64,
    pub iat: Option<u64>,
    pub nonce: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[cfg(feature = "jwt")]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RawIdTokenClaims {
    iss: String,
    sub: String,
    aud: AudienceClaim,
    exp: u64,
    #[serde(default)]
    iat: Option<u64>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    picture: Option<String>,
}

#[cfg(feature = "jwt")]
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

#[cfg(feature = "jwt")]
impl From<RawIdTokenClaims> for IdTokenClaims {
    fn from(value: RawIdTokenClaims) -> Self {
        Self {
            iss: value.iss,
            sub: value.sub,
            aud: match value.aud {
                AudienceClaim::One(value) => vec![value],
                AudienceClaim::Many(value) => value,
            },
            exp: value.exp,
            iat: value.iat,
            nonce: value.nonce,
            email: value.email,
            email_verified: value.email_verified,
            name: value.name,
            picture: value.picture,
        }
    }
}

impl AccessTokenClaims {
    pub fn for_user(
        issuer: impl Into<String>,
        subject: UserId,
        audience: Vec<String>,
        expires_at: u64,
        issued_at: u64,
        token_id: impl Into<String>,
    ) -> Self {
        Self {
            iss: issuer.into(),
            sub: subject.into_string(),
            aud: audience,
            exp: expires_at,
            iat: issued_at,
            jti: token_id.into(),
            tenant_id: None,
            session_id: None,
            roles: Vec::new(),
            scope: Vec::new(),
            auth_time: None,
            extra: BTreeMap::new(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JwksDocument {
    pub keys: Vec<JwksKey>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JwksKey {
    #[cfg_attr(feature = "serde", serde(default))]
    pub kid: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub kty: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub alg: String,
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_jwks_key_use", rename = "use")
    )]
    pub use_: String,
    #[cfg_attr(feature = "serde", serde(default, flatten))]
    pub public_parameters: BTreeMap<String, String>,
}

fn default_jwks_key_use() -> String {
    "sig".to_string()
}

pub fn jwks_key_by_id<'a>(jwks: &'a JwksDocument, key_id: &str) -> Result<&'a JwksKey, AuthError> {
    jwks.keys
        .iter()
        .find(|key| key.kid == key_id)
        .ok_or(AuthError::InvalidToken)
}

pub fn reject_revoked_session(
    claims: &AccessTokenClaims,
    revoked_session_ids: &[&str],
) -> Result<(), AuthError> {
    let Some(session_id) = claims.session_id.as_ref() else {
        return Ok(());
    };
    if revoked_session_ids
        .iter()
        .any(|revoked| session_id.as_str() == *revoked)
    {
        return Err(AuthError::SessionRevoked);
    }
    Ok(())
}

#[cfg(feature = "jwt")]
pub fn encode_access_token(
    claims: &AccessTokenClaims,
    encoding_key: &EncodingKey,
    algorithm: Algorithm,
    key_id: Option<&str>,
) -> Result<String, AuthError> {
    let mut header = Header::new(algorithm);
    header.kid = key_id.map(ToOwned::to_owned);
    encode(&header, claims, encoding_key).map_err(|_| AuthError::InvalidToken)
}

#[cfg(feature = "jwt")]
pub fn decode_access_token(
    token: &str,
    decoding_key: &DecodingKey,
    issuer: &str,
    audience: &str,
    algorithms: &[Algorithm],
) -> Result<AccessTokenClaims, AuthError> {
    let Some(default_algorithm) = algorithms.first().copied() else {
        return Err(AuthError::validation(
            "at least one JWT algorithm is required",
        ));
    };
    let mut validation = Validation::new(default_algorithm);
    validation.algorithms = algorithms.to_vec();
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);

    decode::<AccessTokenClaims>(token, decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(jwt_error_to_auth_error)
}

#[cfg(feature = "jwt")]
pub fn decode_id_token(
    token: &str,
    decoding_key: &DecodingKey,
    issuer: &str,
    audience: &str,
    algorithms: &[Algorithm],
    expected_nonce: Option<&str>,
) -> Result<IdTokenClaims, AuthError> {
    let Some(default_algorithm) = algorithms.first().copied() else {
        return Err(AuthError::validation(
            "at least one JWT algorithm is required",
        ));
    };
    let mut validation = Validation::new(default_algorithm);
    validation.algorithms = algorithms.to_vec();
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);

    let claims = decode::<RawIdTokenClaims>(token, decoding_key, &validation)
        .map(|data| IdTokenClaims::from(data.claims))
        .map_err(jwt_error_to_auth_error)?;

    if claims.sub.trim().is_empty() {
        return Err(AuthError::InvalidToken);
    }
    if let Some(expected_nonce) = expected_nonce {
        let actual_nonce = claims.nonce.as_deref().unwrap_or_default();
        if actual_nonce != expected_nonce {
            return Err(AuthError::InvalidToken);
        }
    }

    Ok(claims)
}

#[cfg(feature = "jwt")]
pub fn access_token_key_id(token: &str) -> Result<String, AuthError> {
    jwt_key_id(token)
}

#[cfg(feature = "jwt")]
pub fn jwt_key_id(token: &str) -> Result<String, AuthError> {
    decode_header(token)
        .ok()
        .and_then(|header| header.kid)
        .filter(|kid| !kid.trim().is_empty())
        .ok_or(AuthError::InvalidToken)
}

#[cfg(feature = "jwt")]
fn jwt_error_to_auth_error(error: jsonwebtoken::errors::Error) -> AuthError {
    match error.kind() {
        JwtErrorKind::ExpiredSignature => AuthError::SessionExpired,
        _ => AuthError::InvalidToken,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwks_key_lookup_returns_matching_key() {
        let jwks = JwksDocument {
            keys: vec![JwksKey {
                kid: "kid-1".to_string(),
                kty: "oct".to_string(),
                alg: "HS256".to_string(),
                use_: "sig".to_string(),
                public_parameters: BTreeMap::new(),
            }],
        };

        assert_eq!(jwks_key_by_id(&jwks, "kid-1").unwrap().kid, "kid-1");
        assert_eq!(
            jwks_key_by_id(&jwks, "unknown").unwrap_err(),
            AuthError::InvalidToken
        );
    }

    #[test]
    fn revoked_session_rejects_matching_session_id() {
        let mut claims = AccessTokenClaims::for_user(
            "issuer",
            UserId::from("user-1"),
            vec!["audience".to_string()],
            4_102_444_800,
            1,
            "token-1",
        );
        claims.session_id = Some(SessionId::from("session-1"));

        assert_eq!(
            reject_revoked_session(&claims, &["session-1"]).unwrap_err(),
            AuthError::SessionRevoked
        );
        assert!(reject_revoked_session(&claims, &["session-2"]).is_ok());
    }

    #[cfg(feature = "json")]
    #[test]
    fn jwks_key_serializes_as_standard_public_jwk_shape() {
        let jwks = JwksDocument {
            keys: vec![JwksKey {
                kid: "kid-1".to_string(),
                kty: "RSA".to_string(),
                alg: "RS256".to_string(),
                use_: "sig".to_string(),
                public_parameters: BTreeMap::from([
                    ("n".to_string(), "modulus".to_string()),
                    ("e".to_string(), "AQAB".to_string()),
                ]),
            }],
        };

        let value = serde_json::to_value(&jwks).unwrap();
        assert_eq!(value["keys"][0]["kid"], "kid-1");
        assert_eq!(value["keys"][0]["use"], "sig");
        assert_eq!(value["keys"][0]["n"], "modulus");
        assert!(value["keys"][0].get("use_").is_none());
        assert!(value["keys"][0].get("public_parameters").is_none());
    }

    #[cfg(feature = "jwt")]
    mod jwt_tests {
        use super::*;
        use jsonwebtoken::{decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header};
        use std::time::{SystemTime, UNIX_EPOCH};

        fn now_seconds() -> u64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }

        fn valid_claims() -> AccessTokenClaims {
            let now = now_seconds();
            let mut claims = AccessTokenClaims::for_user(
                "https://issuer.example",
                UserId::from("user-1"),
                vec!["auth-stack".to_string()],
                now + 300,
                now,
                "token-1",
            );
            claims.tenant_id = Some(TenantId::from("tenant:default"));
            claims.session_id = Some(SessionId::from("session-1"));
            claims.scope = vec!["auth:session:read".to_string()];
            claims
        }

        fn encode_fixture(claims: &AccessTokenClaims, secret: &[u8]) -> String {
            encode_access_token(
                claims,
                &EncodingKey::from_secret(secret),
                Algorithm::HS256,
                Some("kid-1"),
            )
            .unwrap()
        }

        fn encode_id_token_fixture(
            audience: AudienceClaim,
            nonce: Option<&str>,
            secret: &[u8],
        ) -> String {
            let now = now_seconds();
            let claims = RawIdTokenClaims {
                iss: "https://issuer.example".to_string(),
                sub: "provider-subject-1".to_string(),
                aud: audience,
                exp: now + 300,
                iat: Some(now),
                nonce: nonce.map(ToOwned::to_owned),
                email: Some("owner@example.test".to_string()),
                email_verified: Some(true),
                name: Some("Workspace Owner".to_string()),
                picture: None,
            };
            let mut header = Header::new(Algorithm::HS256);
            header.kid = Some("kid-1".to_string());
            encode(&header, &claims, &EncodingKey::from_secret(secret)).unwrap()
        }

        #[test]
        fn jwt_access_token_round_trips_and_sets_key_id() {
            let claims = valid_claims();
            let token = encode_fixture(&claims, b"secret");
            let header = decode_header(&token).unwrap();

            assert_eq!(header.kid.as_deref(), Some("kid-1"));
            assert_eq!(access_token_key_id(&token).unwrap(), "kid-1");
            assert_eq!(jwt_key_id(&token).unwrap(), "kid-1");

            let decoded = decode_access_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
            )
            .unwrap();

            assert_eq!(decoded.sub, "user-1");
            assert_eq!(decoded.session_id, Some(SessionId::from("session-1")));
        }

        #[test]
        fn jwt_access_token_rejects_expired_tokens() {
            let mut claims = valid_claims();
            claims.exp = 1;
            claims.iat = 1;
            let token = encode_fixture(&claims, b"secret");

            let error = decode_access_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
            )
            .unwrap_err();

            assert_eq!(error, AuthError::SessionExpired);
        }

        #[test]
        fn jwt_access_token_rejects_wrong_issuer_or_audience() {
            let claims = valid_claims();
            let token = encode_fixture(&claims, b"secret");

            let wrong_issuer = decode_access_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://other-issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
            )
            .unwrap_err();
            let wrong_audience = decode_access_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "other-audience",
                &[Algorithm::HS256],
            )
            .unwrap_err();

            assert_eq!(wrong_issuer, AuthError::InvalidToken);
            assert_eq!(wrong_audience, AuthError::InvalidToken);
        }

        #[test]
        fn jwt_access_token_rejects_wrong_key() {
            let claims = valid_claims();
            let token = encode_fixture(&claims, b"secret");

            let error = decode_access_token(
                &token,
                &DecodingKey::from_secret(b"other-secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
            )
            .unwrap_err();

            assert_eq!(error, AuthError::InvalidToken);
        }

        #[test]
        fn oidc_id_token_accepts_string_audience_and_nonce() {
            let token = encode_id_token_fixture(
                AudienceClaim::One("auth-stack".to_string()),
                Some("nonce-1"),
                b"secret",
            );

            let claims = decode_id_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
                Some("nonce-1"),
            )
            .unwrap();

            assert_eq!(claims.sub, "provider-subject-1");
            assert_eq!(claims.aud, vec!["auth-stack"]);
            assert_eq!(claims.email.as_deref(), Some("owner@example.test"));
        }

        #[test]
        fn oidc_id_token_accepts_array_audience() {
            let token = encode_id_token_fixture(
                AudienceClaim::Many(vec!["auth-stack".to_string(), "other-client".to_string()]),
                None,
                b"secret",
            );

            let claims = decode_id_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
                None,
            )
            .unwrap();

            assert_eq!(claims.aud.len(), 2);
        }

        #[test]
        fn oidc_id_token_rejects_nonce_mismatch() {
            let token = encode_id_token_fixture(
                AudienceClaim::One("auth-stack".to_string()),
                Some("nonce-1"),
                b"secret",
            );

            let error = decode_id_token(
                &token,
                &DecodingKey::from_secret(b"secret"),
                "https://issuer.example",
                "auth-stack",
                &[Algorithm::HS256],
                Some("nonce-2"),
            )
            .unwrap_err();

            assert_eq!(error, AuthError::InvalidToken);
        }
    }
}
