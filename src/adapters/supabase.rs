use super::http::{truncate_body_for_error, wasi_http_post};

// -------------------------------------------------------------------------
// Supabase PostgREST RPC HTTP adapter
// -------------------------------------------------------------------------

/// RPC contract for Supabase SQL execution.
///
/// The `execute_sql` function must accept:
/// - `query_text`: SQL with `$1`, `$2`, … placeholders
/// - `query_params`: positional parameter values bound server-side
///
/// Client-side interpolation is not used; callers pass parameterized SQL only.
pub const SUPABASE_EXECUTE_SQL_RPC: &str = "execute_sql";

#[cfg(feature = "wasi-supabase-rpc")]
/// Execute SQL via Supabase PostgREST [`SUPABASE_EXECUTE_SQL_RPC`] RPC.
///
/// Positional `$n` placeholders in `sql` are bound through `query_params`.
/// The RPC must be deployed with server-side parameter binding; see
/// [`SUPABASE_EXECUTE_SQL_RPC`] for the expected contract.
pub async fn execute_supabase_query(
    url: &str,
    secret_key: Option<&str>,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let rpc_url = if url.ends_with(&format!("/rest/v1/rpc/{SUPABASE_EXECUTE_SQL_RPC}")) {
        url.to_string()
    } else {
        format!(
            "{}/rest/v1/rpc/{SUPABASE_EXECUTE_SQL_RPC}",
            url.trim_end_matches('/')
        )
    };

    let req_payload = serde_json::json!({
        "query_text": sql,
        "query_params": params,
    });
    let body_data = serde_json::to_vec(&req_payload).map_err(|e| e.to_string())?;

    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(key) = secret_key {
        headers.push(("apikey".to_string(), key.to_string()));
        headers.push(("Authorization".to_string(), format!("Bearer {}", key)));
    }

    let resp_bytes = wasi_http_post(&rpc_url, headers, body_data).await?;
    let resp_val: serde_json::Value = serde_json::from_slice(&resp_bytes).map_err(|error| {
        format!(
            "Failed to parse Supabase response JSON: {error}; body={}",
            truncate_body_for_error(&resp_bytes)
        )
    })?;

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
