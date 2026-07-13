#[cfg(feature = "auth")]
use crate::error::CounterAppError;
use crate::error::CounterAppResult;

#[cfg(feature = "auth")]
#[derive(Clone, Debug, Default)]
pub struct CounterAuthContext {
    authorization: Option<String>,
    session_id: Option<String>,
    request_id: Option<String>,
}

#[cfg(not(feature = "auth"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct CounterAuthContext;

impl CounterAuthContext {
    pub fn from_http_headers(headers: &http::HeaderMap) -> Self {
        #[cfg(not(feature = "auth"))]
        {
            let _ = headers;
            Self
        }
        #[cfg(feature = "auth")]
        {
            Self {
                authorization: header_text(headers, http::header::AUTHORIZATION.as_str()),
                session_id: session_id_from_cookie(headers),
                request_id: header_text(headers, "x-request-id"),
            }
        }
    }

    #[cfg(all(feature = "spin-grpc", runtime_spin))]
    pub fn from_grpc_metadata(metadata: &tonic::metadata::MetadataMap) -> Self {
        #[cfg(not(feature = "auth"))]
        {
            let _ = metadata;
            Self
        }
        #[cfg(feature = "auth")]
        {
            Self {
                authorization: metadata_text(metadata, "authorization"),
                session_id: None,
                request_id: metadata_text(metadata, "x-request-id"),
            }
        }
    }

    pub async fn authorize(&self, permission: &str) -> CounterAppResult<()> {
        #[cfg(not(feature = "auth"))]
        {
            let _ = permission;
            Ok(())
        }

        #[cfg(feature = "auth")]
        {
            self.authorize_with_fullstack(permission).await
        }
    }

    #[cfg(feature = "auth")]
    async fn authorize_with_fullstack(&self, permission: &str) -> CounterAppResult<()> {
        use http_body_util::BodyExt as _;
        use spin_sdk::http::FullBody;

        let canonical_permission = match permission {
            "counter.change" => wasi_auth::authentication::permissions::COUNTER_CHANGE,
            "counter.reset" => wasi_auth::authentication::permissions::COUNTER_RESET,
            _ => return Err(CounterAppError::Forbidden),
        };
        if self.authorization.is_none() && self.session_id.is_none() {
            return Err(CounterAppError::AuthRequired);
        }
        let base_url = counter_auth_base_url().await?;
        let body = serde_json::to_vec(&serde_json::json!({
            "action": canonical_permission,
            "resource_type": "counter",
            "resource_id": "global",
        }))
        .map_err(|error| CounterAppError::serialization(error.to_string()))?;
        let mut builder = spin_sdk::http::Request::post(format!(
            "{}/api/authorization/check",
            base_url.trim_end_matches('/')
        ))
        .header(http::header::CONTENT_TYPE, "application/json");
        if let Some(value) = &self.authorization {
            builder = builder.header(http::header::AUTHORIZATION, value);
        }
        if let Some(value) = &self.session_id {
            builder = builder.header(http::header::COOKIE, format!("__Host-session={value}"));
        }
        if let Some(value) = &self.request_id {
            builder = builder.header("x-request-id", value);
        }
        let request = builder
            .body(FullBody::new(bytes::Bytes::from(body)))
            .map_err(|error| CounterAppError::transport(error.to_string()))?;
        let response = spin_sdk::http::send(request).await.map_err(|error| {
            CounterAppError::transport(format!("authorization transport failed: {error}"))
        })?;
        if response.status() == http::StatusCode::UNAUTHORIZED {
            return Err(CounterAppError::AuthRequired);
        }
        if !response.status().is_success() {
            return Err(CounterAppError::Forbidden);
        }
        let bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|error| {
                CounterAppError::transport(format!("authorization response failed: {error:?}"))
            })?
            .to_bytes();
        let allowed = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| value.get("allowed").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);
        if allowed {
            Ok(())
        } else {
            Err(CounterAppError::Forbidden)
        }
    }
}

#[cfg(feature = "auth")]
fn header_text(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "auth")]
fn session_id_from_cookie(headers: &http::HeaderMap) -> Option<String> {
    let cookies = header_text(headers, http::header::COOKIE.as_str())?;
    cookies.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        matches!(name, "__Host-session" | "wasi_auth_dev_session")
            .then(|| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

#[cfg(all(feature = "auth", feature = "spin-grpc", runtime_spin))]
fn metadata_text(metadata: &tonic::metadata::MetadataMap, name: &str) -> Option<String> {
    metadata
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(feature = "auth")]
async fn counter_auth_base_url() -> CounterAppResult<String> {
    if let Ok(value) = spin_sdk::variables::get("counter_auth_base_url").await
        && !value.trim().is_empty()
    {
        return Ok(value);
    }
    std::env::var("COUNTER_AUTH_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CounterAppError::configuration("COUNTER_AUTH_BASE_URL is not configured"))
}
