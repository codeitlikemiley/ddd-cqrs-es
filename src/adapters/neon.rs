#[cfg(feature = "wasi-neon")]
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

use super::http::wasi_http_post;

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

    let http_url = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let stripped = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))
            .unwrap_or(url);
        let host_part = if let Some(at_idx) = stripped.find('@') {
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
        format!("https://{}/sql", host_name)
    } else {
        url.to_string()
    };

    let resp_bytes = wasi_http_post(&http_url, headers, body_data).await?;

    let resp_val: serde_json::Value = serde_json::from_slice(&resp_bytes)
        .map_err(|e| format!("Failed to parse Neon response JSON: {}", e))?;

    let mut parsed_rows = Vec::new();
    if let Some(arr) = resp_val.as_array() {
        parsed_rows = arr.clone();
    } else if let Some(obj) = resp_val.as_object() {
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
            }
        }
    }

    Ok(parsed_rows)
}
