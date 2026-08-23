// -------------------------------------------------------------------------
// WASI HTTP Outbound post client helper
// -------------------------------------------------------------------------
#[cfg(feature = "wasi-http")]
pub async fn wasi_http_post(
    url: &str,
    headers: Vec<(String, String)>,
    body_data: Vec<u8>,
) -> Result<Vec<u8>, String> {
    use http_body_util::BodyExt;
    let body = http_body_util::Full::new(bytes::Bytes::from(body_data));
    let mut req_builder = http::Request::builder().method("POST").uri(url);

    for (name, value) in headers {
        req_builder = req_builder.header(name, value);
    }

    let req = req_builder
        .body(body)
        .map_err(|e| format!("Failed to build HTTP request: {:?}", e))?;

    let wasi_req = wasip3::http_compat::http_into_wasi_request(req)
        .map_err(|e| format!("Failed to convert to WASI request: {:?}", e))?;

    let wasi_resp = wasip3::http::client::send(wasi_req)
        .await
        .map_err(|e| format!("WASI HTTP send error: {:?}", e))?;

    let http_resp = wasip3::http_compat::http_from_wasi_response(wasi_resp)
        .map_err(|e| format!("Failed to convert from WASI response: {:?}", e))?;

    let status = http_resp.status();
    let body_bytes = http_resp
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("Failed to collect response body: {:?}", e))?
        .to_bytes()
        .to_vec();

    if !status.is_success() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        return Err(format!(
            "HTTP request failed with status {}: {}",
            status, body_str
        ));
    }

    Ok(body_bytes)
}
