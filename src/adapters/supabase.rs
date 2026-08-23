use super::http::wasi_http_post;
use super::sql_text::interpolate_query;

// -------------------------------------------------------------------------
// Supabase PostgREST RPC HTTP adapter
// -------------------------------------------------------------------------
#[cfg(feature = "wasi-supabase-rpc")]
/// Execute SQL via Supabase PostgREST `execute_sql` RPC.
///
/// This helper interpolates the SQL template and extracts RPC SQL errors into
/// readable string failures.
pub async fn execute_supabase_query(
    url: &str,
    secret_key: Option<&str>,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let rpc_url = if url.ends_with("/rest/v1/rpc/execute_sql") {
        url.to_string()
    } else {
        format!("{}/rest/v1/rpc/execute_sql", url.trim_end_matches('/'))
    };

    let interpolated_sql = interpolate_query(sql, &params)?;

    let req_payload = serde_json::json!({
        "query_text": interpolated_sql,
        "query_params": Vec::<serde_json::Value>::new(),
    });
    let body_data = serde_json::to_vec(&req_payload).map_err(|e| e.to_string())?;

    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(key) = secret_key {
        headers.push(("apikey".to_string(), key.to_string()));
        headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
    }

    let resp_bytes = wasi_http_post(&rpc_url, headers, body_data).await?;
    let resp_val: serde_json::Value = serde_json::from_slice(&resp_bytes)
        .map_err(|e| format!("Failed to parse Supabase response: {}", e))?;

    if let Some(err_obj) = resp_val.as_object() {
        if let Some(err_msg) = err_obj.get("error") {
            return Err(format!("Supabase SQL error: {}", err_msg));
        }

        if let Some(message) = err_obj.get("message").and_then(|v| v.as_str()) {
            let code = err_obj
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let details = err_obj
                .get("details")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(|v| format!(" details: {}", v))
                .unwrap_or_default();
            let hint = err_obj
                .get("hint")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(|v| format!(" hint: {}", v))
                .unwrap_or_default();

            return Err(format!(
                "Supabase SQL error [{}]: {}{}{}",
                code, message, details, hint
            ));
        }
    }

    let rows = resp_val
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![resp_val]);

    Ok(rows)
}
