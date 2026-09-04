// -------------------------------------------------------------------------
// WASI HTTP Outbound post client helper
// -------------------------------------------------------------------------

use std::time::{Duration, Instant};

/// Maximum request body size accepted by [`wasi_http_post`].
pub const MAX_HTTP_REQUEST_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Maximum response body size collected by [`wasi_http_post`].
pub const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Wall-clock deadline applied to outbound WASI HTTP calls.
pub const HTTP_REQUEST_DEADLINE: Duration = Duration::from_secs(30);

const ERROR_BODY_SNIPPET_MAX: usize = 512;

/// Returns an error when `url` is not HTTPS, except for loopback HTTP used in
/// local development.
pub fn validate_https_url(url: &str) -> Result<(), String> {
    let scheme_end = url.find("://").ok_or_else(|| {
        format!("HTTP URL must include a scheme (got {})", redact_url_userinfo(url))
    })?;
    let scheme = &url[..scheme_end];
    if scheme.eq_ignore_ascii_case("https") {
        return Ok(());
    }
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(format!(
            "unsupported URL scheme `{scheme}`; only https is allowed"
        ));
    }

    let authority = url[scheme_end + 3..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host = authority
        .rsplit('@')
        .next()
        .unwrap_or(authority)
        .trim_matches(['[', ']']);
    let host = host.split(':').next().unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
    {
        Ok(())
    } else {
        Err(format!(
            "refusing plaintext HTTP to `{host}`; use https or a loopback URL"
        ))
    }
}

/// Redacts userinfo from a URL for logs and error strings.
pub fn redact_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let scheme = &url[..scheme_end + 3];
    let remainder = &url[scheme_end + 3..];
    let Some(at_idx) = remainder.rfind('@') else {
        return url.to_string();
    };
    let host_and_path = &remainder[at_idx + 1..];
    format!("{scheme}***@{host_and_path}")
}

/// Formats a response body snippet for error messages without dumping secrets.
pub fn truncate_body_for_error(body: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(body);
    if lossy.len() <= ERROR_BODY_SNIPPET_MAX {
        lossy.into_owned()
    } else {
        format!(
            "{}… ({} bytes total)",
            &lossy[..ERROR_BODY_SNIPPET_MAX],
            body.len()
        )
    }
}

fn ensure_within_deadline(started: Instant, phase: &str) -> Result<(), String> {
    if started.elapsed() > HTTP_REQUEST_DEADLINE {
        return Err(format!(
            "HTTP request exceeded {:?} deadline during {phase}",
            HTTP_REQUEST_DEADLINE
        ));
    }
    Ok(())
}

#[cfg(feature = "wasi-http")]
pub async fn wasi_http_post(
    url: &str,
    headers: Vec<(String, String)>,
    body_data: Vec<u8>,
) -> Result<Vec<u8>, String> {
    use http_body_util::BodyExt;

    validate_https_url(url)?;
    if body_data.len() > MAX_HTTP_REQUEST_BODY_BYTES {
        return Err(format!(
            "HTTP request body exceeds {} byte limit",
            MAX_HTTP_REQUEST_BODY_BYTES
        ));
    }

    let started = Instant::now();
    let body = http_body_util::Full::new(bytes::Bytes::from(body_data));
    let mut req_builder = http::Request::builder().method("POST").uri(url);

    for (name, value) in headers {
        req_builder = req_builder.header(name, value);
    }

    let req = req_builder
        .body(body)
        .map_err(|error| format!("Failed to build HTTP request: {error:?}"))?;

    let wasi_req = wasip3::http_compat::http_into_wasi_request(req)
        .map_err(|error| format!("Failed to convert to WASI request: {error:?}"))?;

    let wasi_resp = wasip3::http::client::send(wasi_req)
        .await
        .map_err(|error| format!("WASI HTTP send error: {error:?}"))?;
    ensure_within_deadline(started, "send")?;

    let http_resp = wasip3::http_compat::http_from_wasi_response(wasi_resp)
        .map_err(|error| format!("Failed to convert from WASI response: {error:?}"))?;

    let status = http_resp.status();
    let body_bytes = http_resp
        .into_body()
        .collect()
        .await
        .map_err(|error| format!("Failed to collect response body: {error:?}"))?
        .to_bytes()
        .to_vec();
    ensure_within_deadline(started, "response body")?;

    if body_bytes.len() > MAX_HTTP_RESPONSE_BODY_BYTES {
        return Err(format!(
            "HTTP response body exceeds {} byte limit",
            MAX_HTTP_RESPONSE_BODY_BYTES
        ));
    }

    if !status.is_success() {
        return Err(format!(
            "HTTP request failed with status {}: {}",
            status,
            truncate_body_for_error(&body_bytes)
        ));
    }

    Ok(body_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_plaintext_remote_http() {
        assert!(validate_https_url("http://example.com/path").is_err());
        assert!(validate_https_url("http://evil.test:8080").is_err());
    }

    #[test]
    fn allows_https_and_loopback_http() {
        assert!(validate_https_url("https://api.example.com/sql").is_ok());
        assert!(validate_https_url("http://127.0.0.1:8080/v2/pipeline").is_ok());
        assert!(validate_https_url("http://localhost/v2/pipeline").is_ok());
    }

    #[test]
    fn redacts_userinfo_from_urls() {
        assert_eq!(
            redact_url_userinfo("postgres://user:secret@db.example.com/neon"),
            "postgres://***@db.example.com/neon"
        );
    }

    #[test]
    fn truncates_error_bodies() {
        let body = vec![b'x'; ERROR_BODY_SNIPPET_MAX + 10];
        let snippet = truncate_body_for_error(&body);
        assert!(snippet.contains("bytes total"));
        assert!(snippet.len() <= ERROR_BODY_SNIPPET_MAX + 32);
    }
}
