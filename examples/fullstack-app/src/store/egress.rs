//! Outbound URL / host validation for dashboard connectors (SSRF guard).

use std::net::IpAddr;

use url::Url;

use crate::error::{AuthStackError, AuthStackResult};

const MAX_URL_LEN: usize = 2_048;

/// Reject private / link-local / metadata hosts unless `allow_private`.
pub fn validate_http_url(url: &str, allow_private: bool) -> AuthStackResult<()> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AuthStackError::validation("url is required"));
    }
    if trimmed.len() > MAX_URL_LEN {
        return Err(AuthStackError::validation("url is too long"));
    }
    let parsed = Url::parse(trimmed).map_err(|_| AuthStackError::validation("url is invalid"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(AuthStackError::validation("url must be http(s)")),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AuthStackError::validation(
            "url must not embed credentials (use vault secrets instead)",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| AuthStackError::validation("url host is missing"))?;
    validate_host(host, allow_private)?;
    resolve_host_if_available(host, allow_private)?;
    Ok(())
}

/// Validate a hostname or IP literal (Postgres / gRPC connectors).
pub fn validate_postgres_host(host: &str, allow_private: bool) -> AuthStackResult<()> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AuthStackError::validation("postgres host is required"));
    }
    if host.len() > 253 {
        return Err(AuthStackError::validation("postgres host is too long"));
    }
    validate_host(host, allow_private)?;
    resolve_host_if_available(host, allow_private)?;
    Ok(())
}

/// Validate a gRPC host literal (native path; gateway URLs use [`validate_http_url`]).
pub fn validate_grpc_host(host: &str, allow_private: bool) -> AuthStackResult<()> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AuthStackError::validation("grpc host is required"));
    }
    validate_host(host, allow_private)?;
    resolve_host_if_available(host, allow_private)?;
    Ok(())
}

pub(crate) fn validate_host(host: &str, allow_private: bool) -> AuthStackResult<()> {
    let host = host.trim().trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() {
        return Err(AuthStackError::validation("host is missing"));
    }
    if host.contains('/') || host.contains('\\') {
        return Err(AuthStackError::validation("host is invalid"));
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        if !allow_private {
            return Err(AuthStackError::validation(
                "localhost targets are blocked (set AUTH_DASHBOARD_HTTP_ALLOW_PRIVATE=true to allow)",
            ));
        }
        return Ok(());
    }
    for suffix in [".internal", ".local", ".home.arpa"] {
        if lower.ends_with(suffix) {
            if !allow_private {
                return Err(AuthStackError::validation(format!(
                    "private suffix host `{host}` is blocked"
                )));
            }
            return Ok(());
        }
    }
    if lower == "metadata.google.internal" || lower.ends_with(".metadata.google.internal") {
        if !allow_private {
            return Err(AuthStackError::validation(
                "cloud metadata hosts are blocked",
            ));
        }
        return Ok(());
    }
    if let Some(ip) = normalize_ip_literal(host) {
        if is_blocked_ip(ip) && !allow_private {
            return Err(AuthStackError::validation(
                "private or link-local IP targets are blocked",
            ));
        }
        return Ok(());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(ip) && !allow_private {
            return Err(AuthStackError::validation(
                "private or link-local IP targets are blocked",
            ));
        }
    }
    Ok(())
}

/// Normalize decimal/hex IPv4 literals (e.g. `2130706433` → 127.0.0.1) for SSRF checks.
pub(crate) fn normalize_ip_literal(host: &str) -> Option<IpAddr> {
    let host = host.trim();
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    if host.starts_with("0x") || host.starts_with("0X") {
        if let Ok(n) = u32::from_str_radix(&host[2..], 16) {
            return Some(IpAddr::V4(std::net::Ipv4Addr::from(n)));
        }
    }
    if host.chars().all(|c| c.is_ascii_digit()) && !host.is_empty() {
        host.parse::<u32>()
            .ok()
            .map(std::net::Ipv4Addr::from)
            .map(IpAddr::V4)
    } else {
        None
    }
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unspecified()
                || v6.is_multicast()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_host_if_available(host: &str, allow_private: bool) -> AuthStackResult<()> {
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    use std::net::ToSocketAddrs;
    let resolved = match (host, 0).to_socket_addrs() {
        Ok(addrs) => addrs.map(|addr| addr.ip()).collect::<Vec<_>>(),
        Err(_) => return Ok(()),
    };
    for ip in resolved {
        if is_blocked_ip(ip) && !allow_private {
            return Err(AuthStackError::validation(format!(
                "host `{host}` resolves to a blocked address"
            )));
        }
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn resolve_host_if_available(_host: &str, _allow_private: bool) -> AuthStackResult<()> {
    // Spin/WASI has no portable DNS from guest code; rely on spin.toml egress allowlists.
    Ok(())
}
