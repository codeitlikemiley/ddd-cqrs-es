use super::http::wasi_http_post;

// -------------------------------------------------------------------------
// Turso / LibSQL Hrana /v2/pipeline HTTP adapter
// -------------------------------------------------------------------------
#[cfg(feature = "wasi-libsql")]
/// Container for libSQL pipeline results including rowset and optional last insert id.
pub struct LibSqlResult {
    /// Parsed rowset from the `/v2/pipeline` response.
    pub rows: Vec<serde_json::Value>,
    /// Optional last insert row id when supported by the executed statement.
    pub last_insert_rowid: Option<u64>,
}

#[cfg(feature = "wasi-libsql")]
fn to_hrana_arg(val: serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Null => serde_json::json!({ "type": "null" }),
        serde_json::Value::Bool(b) => {
            serde_json::json!({ "type": "integer", "value": if b { "1" } else { "0" } })
        }
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::json!({ "type": "integer", "value": i.to_string() })
            } else if let Some(f) = n.as_f64() {
                serde_json::json!({ "type": "float", "value": f })
            } else {
                serde_json::json!({ "type": "null" })
            }
        }
        serde_json::Value::String(s) => serde_json::json!({ "type": "text", "value": s }),
        _ => serde_json::json!({ "type": "text", "value": val.to_string() }),
    }
}

#[cfg(feature = "wasi-libsql")]
fn from_hrana_val(val: &serde_json::Value) -> serde_json::Value {
    if let Some(t) = val.get("type").and_then(|v| v.as_str()) {
        match t {
            "null" => serde_json::Value::Null,
            "text" => val.get("value").cloned().unwrap_or(serde_json::Value::Null),
            "integer" => {
                if let Some(s) = val.get("value").and_then(|v| v.as_str()) {
                    if let Ok(i) = s.parse::<i64>() {
                        serde_json::Value::Number(serde_json::Number::from(i))
                    } else {
                        serde_json::Value::Null
                    }
                } else {
                    serde_json::Value::Null
                }
            }
            "float" => val.get("value").cloned().unwrap_or(serde_json::Value::Null),
            "blob" => val
                .get("base64")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Null,
        }
    } else {
        serde_json::Value::Null
    }
}

#[cfg(feature = "wasi-libsql")]
fn parse_libsql_result(resp: &serde_json::Value) -> Result<LibSqlResult, String> {
    let results = resp
        .get("results")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "Missing results array in LibSQL response".to_string())?;

    for res in results {
        if let Some(t) = res.get("type").and_then(|v| v.as_str()) {
            if t == "error" {
                let msg = res
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error");
                return Err(format!("LibSQL query error: {}", msg));
            }
        }

        if let Some(response) = res.get("response") {
            if let Some(result) = response.get("result") {
                let cols = result
                    .get("cols")
                    .and_then(|c| c.as_array())
                    .ok_or_else(|| "Missing cols in LibSQL execute result".to_string())?;

                let rows_array = result
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .ok_or_else(|| "Missing rows in LibSQL execute result".to_string())?;

                let last_insert_rowid = result.get("last_insert_rowid").and_then(|v| {
                    if let Some(s) = v.as_str() {
                        s.parse::<u64>().ok()
                    } else {
                        v.as_u64()
                    }
                });

                let col_names: Vec<String> = cols
                    .iter()
                    .map(|c| {
                        c.get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect();

                let mut rows = Vec::new();
                for r in rows_array {
                    let r_arr = r
                        .as_array()
                        .ok_or_else(|| "Row is not an array".to_string())?;

                    let mut obj = serde_json::Map::new();
                    for (i, val) in r_arr.iter().enumerate() {
                        if let Some(col_name) = col_names.get(i) {
                            obj.insert(col_name.clone(), from_hrana_val(val));
                        }
                    }
                    rows.push(serde_json::Value::Object(obj));
                }
                return Ok(LibSqlResult {
                    rows,
                    last_insert_rowid,
                });
            }
        }
    }

    Ok(LibSqlResult {
        rows: Vec::new(),
        last_insert_rowid: None,
    })
}

#[cfg(feature = "wasi-libsql")]
/// Execute a parameterized statement against a Turso/LibSQL endpoint.
///
/// Rows are converted to JSON objects and a helper result is returned for both
/// query and write command paths.
pub async fn execute_libsql_query(
    url: &str,
    auth_token: Option<&str>,
    sql: &str,
    params: Vec<serde_json::Value>,
) -> Result<LibSqlResult, String> {
    let hrana_args: Vec<serde_json::Value> = params.into_iter().map(to_hrana_arg).collect();

    let req_payload = serde_json::json!({
        "baton": null,
        "requests": [
            {
                "type": "execute",
                "stmt": {
                    "sql": sql,
                    "args": hrana_args
                }
            },
            {
                "type": "close"
            }
        ]
    });

    let body_data = serde_json::to_vec(&req_payload).map_err(|e| e.to_string())?;

    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    if let Some(tok) = auth_token {
        headers.push(("Authorization".to_string(), format!("Bearer {}", tok)));
    }

    let resolved_url = if let Some(rest) = url.strip_prefix("libsql://") {
        format!("https://{}", rest)
    } else {
        url.to_string()
    };

    let pipeline_url = if resolved_url.ends_with("/v2/pipeline") {
        resolved_url
    } else if resolved_url.ends_with('/') {
        format!("{}v2/pipeline", resolved_url)
    } else {
        format!("{}/v2/pipeline", resolved_url)
    };

    let resp_bytes = wasi_http_post(&pipeline_url, headers, body_data).await?;
    let resp_json: serde_json::Value = serde_json::from_slice(&resp_bytes)
        .map_err(|e| format!("Failed to parse LibSQL response: {}", e))?;

    parse_libsql_result(&resp_json)
}
