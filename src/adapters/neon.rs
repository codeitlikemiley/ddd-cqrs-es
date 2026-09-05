#[cfg(feature = "wasi-neon")]
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

use super::http::{redact_url_userinfo, truncate_body_for_error, validate_https_url, wasi_http_post};

// -------------------------------------------------------------------------
// Neon / Serverless Postgres HTTP adapter
// -------------------------------------------------------------------------
#[cfg(feature = "wasi-neon")]
/// Execute a Neon SQL statement via the Neon HTTP API and return decoded rows.
///
/// The query `sql` is sent with positional parameters and results are returned as
/// raw JSON values to keep the adapter transport-agnostic.
pub async fn execute_neon_query(
    url: &str,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let req_payload = serde_json::json!({
        "query": sql,
        "params": params,
    });
    let body_data = serde_json::to_vec(&req_payload).map_err(|e| e.to_string())?;

    let mut headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Neon-Raw-Text-Output".to_string(), "true".to_string()),
        ("Neon-Array-Mode".to_string(), "true".to_string()),
    ];

    let connection_string = env_non_empty("DATABASE_URL")
        .or_else(|| env_non_empty("NEON_DB_URL"))
        .unwrap_or_else(|| url.to_string());
    if !connection_string.is_empty()
        && (connection_string.starts_with("postgres://")
            || connection_string.starts_with("postgresql://"))
    {
        headers.push(("Neon-Connection-String".to_string(), connection_string));
    }

    let http_url = neon_http_endpoint(url)?;

    if let Some(conn_str) = env_non_empty("DATABASE_URL")
        .or_else(|| env_non_empty("NEON_DB_URL"))
        .filter(|value| {
            value.starts_with("postgres://") || value.starts_with("postgresql://")
        })
    {
        if let (Some(conn_host), Some(endpoint_host)) = (
            postgres_connection_host(&conn_str),
            https_endpoint_host(&http_url),
        ) {
            if conn_host != endpoint_host {
                return Err(format!(
                    "Neon connection string host `{conn_host}` does not match endpoint host `{endpoint_host}`"
                ));
            }
        }
    }

    let resp_bytes = wasi_http_post(&http_url, headers, body_data).await?;

    let resp_val: serde_json::Value = serde_json::from_slice(&resp_bytes).map_err(|error| {
        format!(
            "Failed to parse Neon response JSON from {}: {error}; body={}",
            redact_url_userinfo(&http_url),
            truncate_body_for_error(&resp_bytes)
        )
    })?;

    parse_neon_rows(&resp_val)
}

#[cfg(feature = "wasi-neon")]
fn neon_http_endpoint(url: &str) -> Result<String, String> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let stripped = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .unwrap_or(url);
        // Credentials may contain '@'; the host separator is the last '@'.
        let host_part = if let Some(at_idx) = stripped.rfind('@') {
            &stripped[at_idx + 1..]
        } else {
            stripped
        };
        let host = if let Some(slash_idx) = host_part.find('/') {
            &host_part[..slash_idx]
        } else if let Some(query_idx) = host_part.find('?') {
            &host_part[..query_idx]
        } else {
            host_part
        };
        let host_name = if let Some(colon_idx) = host.find(':') {
            &host[..colon_idx]
        } else {
            host
        };
        Ok(format!("https://{host_name}/sql"))
    } else {
        validate_https_url(url)?;
        Ok(url.to_string())
    }
}

#[cfg(feature = "wasi-neon")]
fn postgres_connection_host(connection_string: &str) -> Option<String> {
    let stripped = connection_string
        .strip_prefix("postgres://")
        .or_else(|| connection_string.strip_prefix("postgresql://"))?;
    let host_part = stripped.rfind('@').map_or(stripped, |idx| &stripped[idx + 1..]);
    let host = host_part
        .split(['/', '?'])
        .next()
        .unwrap_or(host_part);
    host.split(':').next().map(str::to_owned)
}

#[cfg(feature = "wasi-neon")]
fn https_endpoint_host(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    if !url[..scheme_end].eq_ignore_ascii_case("https") {
        return None;
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
    host.split(':').next().map(str::to_owned)
}

#[cfg(feature = "wasi-neon")]
fn parse_neon_rows(resp_val: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    if let Some(arr) = resp_val.as_array() {
        return Ok(arr.clone());
    }

    if let Some(obj) = resp_val.as_object() {
        if let (Some(fields_val), Some(rows_val)) = (obj.get("fields"), obj.get("rows")) {
            if let (Some(fields_arr), Some(rows_arr)) = (fields_val.as_array(), rows_val.as_array())
            {
                let col_names: Vec<String> = fields_arr
                    .iter()
                    .map(|f| {
                        f.get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect();

                let mut parsed_rows = Vec::new();
                for row_val in rows_arr {
                    if let Some(row_arr) = row_val.as_array() {
                        let mut row_obj = serde_json::Map::new();
                        for (i, col_val) in row_arr.iter().enumerate() {
                            if i < col_names.len() {
                                row_obj.insert(col_names[i].clone(), col_val.clone());
                            }
                        }
                        parsed_rows.push(serde_json::Value::Object(row_obj));
                    }
                }
                return Ok(parsed_rows);
            }
        }

        if obj.contains_key("message") || obj.contains_key("error") {
            return Err(format!("Neon SQL error: {resp_val}"));
        }
    }

    Err(format!(
        "Neon response did not contain a recognizable rowset: {}",
        truncate_body_for_error(resp_val.to_string().as_bytes())
    ))
}

#[cfg(all(test, feature = "wasi-neon"))]
mod tests {
    use super::*;

    #[test]
    fn neon_endpoint_uses_last_at_as_host_separator() {
        let endpoint = neon_http_endpoint(
            "postgres://user:p@ss@word@ep-example.neon.tech/neondb?sslmode=require",
        )
        .unwrap();
        assert_eq!(endpoint, "https://ep-example.neon.tech/sql");
    }

    #[test]
    fn neon_endpoint_redacts_userinfo_in_errors() {
        let redacted = redact_url_userinfo("postgres://user:secret@ep-example.neon.tech/neondb");
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("***@"));
    }

    #[test]
    fn neon_endpoint_rejects_plaintext_http_urls() {
        assert!(neon_http_endpoint("http://evil.example/sql").is_err());
        assert!(neon_http_endpoint("https://ep-example.neon.tech/sql").is_ok());
    }

    #[test]
    fn postgres_connection_host_uses_last_at_separator() {
        assert_eq!(
            postgres_connection_host("postgres://user:p@ss@word@ep-example.neon.tech/db"),
            Some("ep-example.neon.tech".to_owned())
        );
    }
}
