//! Shared client helpers for app modules.

#![allow(dead_code)]
#![allow(unused_imports)]

use crate::contracts::{LoginCompletionResponse, SessionView};
use leptos::prelude::*;
use server_fn::ServerFnError;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsValue;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "hydrate")]
use web_sys::window;

pub(crate) fn short_id_label(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.len() <= 12 {
        return trimmed.to_owned();
    }
    let head: String = trimmed.chars().take(6).collect();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

pub(crate) fn org_monogram(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut chars = cleaned.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_ascii_uppercase(), b.to_ascii_uppercase()),
        (Some(a), None) => a.to_ascii_uppercase().to_string(),
        _ => "?".to_owned(),
    }
}

pub(crate) fn org_tone_index(name: &str) -> u8 {
    let hash = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(33).wrapping_add(b as u32));
    (hash % 6) as u8
}

/// Pathname for island code (no Router context on hydrate).
pub(crate) fn current_browser_pathname() -> String {
    #[cfg(feature = "hydrate")]
    {
        window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_else(|| "/".to_owned())
    }
    #[cfg(not(feature = "hydrate"))]
    {
        "/".to_owned()
    }
}

/// True when URL has `new=1` / `new=true` (create-workspace intent).
pub(crate) fn current_browser_search_has_new() -> bool {
    #[cfg(feature = "hydrate")]
    {
        let search = window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let q = search.trim_start_matches('?');
        q.split('&').any(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let val = parts.next().unwrap_or("1");
            key == "new"
                && matches!(
                    val.to_ascii_lowercase().as_str(),
                    "" | "1" | "true" | "yes" | "on"
                )
        })
    }
    #[cfg(not(feature = "hydrate"))]
    {
        false
    }
}

#[cfg(feature = "hydrate")]
pub(crate) fn mark_active_nav(pathname: &str) {
    let Some(document) = window().and_then(|window| window.document()) else {
        return;
    };

    let states = [
        ("[data-nav='overview']", pathname == "/dashboard"),
        (
            "[data-nav='organizations']",
            pathname == "/organizations" || pathname.starts_with("/organizations/"),
        ),
        (
            "[data-nav='system']",
            pathname == "/admin" || pathname.starts_with("/admin/"),
        ),
        (
            "[data-nav='settings-general']",
            pathname.ends_with("/settings")
                || pathname.ends_with("/settings/")
                || pathname.ends_with("/settings/general"),
        ),
        (
            "[data-nav='settings-members']",
            pathname.ends_with("/settings/members"),
        ),
        (
            "[data-nav='settings-invitations']",
            pathname.ends_with("/settings/invitations"),
        ),
        (
            "[data-nav='settings-roles']",
            pathname.ends_with("/settings/roles"),
        ),
        (
            "[data-nav='settings-audit']",
            pathname.ends_with("/settings/audit"),
        ),
        (
            "[data-nav='settings-danger']",
            pathname.ends_with("/settings/danger"),
        ),
    ];

    // Clear previous actives on all data-nav links, then set matches.
    if let Ok(nodes) = document.query_selector_all("[data-nav]") {
        for i in 0..nodes.length() {
            if let Some(node) = nodes.item(i) {
                if let Some(el) = node.dyn_ref::<web_sys::Element>() {
                    let _ = el.class_list().remove_1("is-active");
                }
            }
        }
    }

    for (selector, active) in states {
        if active {
            if let Ok(Some(element)) = document.query_selector(selector) {
                let _ = element.class_list().add_1("is-active");
            }
        }
    }
}

pub(crate) fn can_view_system_navigation(session: &SessionView) -> bool {
    session.system_administrator && session.assurance == "aal2"
        || session
            .permissions
            .iter()
            .any(|permission| permission.starts_with("system.") || permission.starts_with("auth:"))
}

pub(crate) fn has_permission(session: &SessionView, permission: &str) -> bool {
    session.permissions.iter().any(|value| value == permission)
}

pub(crate) fn selected_auth_error(
    register_mode: bool,
    login_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
    register_result: Option<Result<LoginCompletionResponse, ServerFnError>>,
) -> Option<String> {
    let selected = if register_mode {
        register_result
    } else {
        login_result
    };
    match selected {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

pub(crate) fn selected_action_error<T>(result: Option<Result<T, ServerFnError>>) -> Option<String> {
    match result {
        Some(Err(error)) => Some(server_error_text(error)),
        _ => None,
    }
}

pub(crate) fn validate_email_only(email: &str) -> Result<(), String> {
    if email.trim().is_empty() {
        return Err("Email is required.".to_string());
    }
    if !email.contains('@') || !email.contains('.') {
        return Err("Enter a valid email address.".to_string());
    }
    Ok(())
}

pub(crate) fn validate_login_form(
    email: &str,
    password: &str,
    register_mode: bool,
) -> Result<(), String> {
    validate_email_only(email)?;
    if password.is_empty() {
        return Err("Password is required.".to_string());
    }
    if register_mode && !(15..=128).contains(&password.chars().count()) {
        return Err("Password must contain 15 to 128 characters.".to_string());
    }
    Ok(())
}

/// Public text for UI banners. Prefer the product message over Leptos
/// `ServerFnError` Display wrappers like `error running server function: …`.
pub(crate) fn server_error_text(error: ServerFnError) -> String {
    // Product/server-fn mapping uses `ServerFnError::new` → `ServerError(msg)`.
    // Matching that variant returns the bare public message. Other variants fall
    // back to Display with known prefixes stripped.
    if let ServerFnError::ServerError(message) = error {
        return message;
    }

    let text = error.to_string();
    const PREFIXES: &[&str] = &[
        "error running server function: ",
        "error reaching server to call server function: ",
        "error generating HTTP response: ",
        "error deserializing server function results: ",
        "error serializing server function arguments: ",
        "error deserializing server function arguments: ",
        "error while trying to register the server function: ",
        "error running middleware: ",
        "missing argument ",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            return rest.to_owned();
        }
    }
    text
}

#[cfg(test)]
mod server_error_text_tests {
    use super::server_error_text;
    use server_fn::ServerFnError;

    #[test]
    fn returns_bare_product_message() {
        let error = ServerFnError::new(r#"workspace URL “goldcoders-corp” is already taken"#);
        assert_eq!(
            error.to_string(),
            r#"error running server function: workspace URL “goldcoders-corp” is already taken"#
        );
        assert_eq!(
            server_error_text(error),
            r#"workspace URL “goldcoders-corp” is already taken"#
        );
    }
}

pub(crate) fn action_result_text<T>(result: Option<Result<T, ServerFnError>>) -> String {
    match result {
        Some(Ok(_)) => "Request accepted".to_string(),
        Some(Err(error)) => server_error_text(error),
        None => String::new(),
    }
}

pub(crate) fn optional_text(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(feature = "hydrate")]
pub(crate) fn passkey_js_string(value: JsValue) -> Result<String, String> {
    value
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Passkey response was not readable.".to_string())
}

#[cfg(feature = "hydrate")]
pub(crate) fn passkey_js_error(error: JsValue) -> String {
    // Prefer plain string throws from the JS layer. Fall back to Debug only
    // for unexpected DOMException / Error objects, then sanitize noise.
    let raw = error
        .as_string()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{error:?}"));
    sanitize_passkey_client_error(&raw)
}

/// Strip wasm-bindgen / browser stack noise so the UI never shows
/// `JsValue(Error: … at createPasskeyCredential …)`.
pub(crate) fn sanitize_passkey_client_error(raw: &str) -> String {
    let mut message = raw.trim().to_owned();
    if message.is_empty() || message == "JsValue(undefined)" {
        return "Passkey prompt was cancelled or unavailable.".to_owned();
    }

    if let Some(inner) = message
        .strip_prefix("JsValue(")
        .and_then(|value| value.strip_suffix(')'))
    {
        message = inner.trim().to_owned();
    }
    if let Some(rest) = message.strip_prefix("Error: ") {
        message = rest.trim().to_owned();
    }
    // Drop trailing stack frames injected by wasm-bindgen Debug formatting.
    for marker in [
        " at createPasskeyCredential",
        " at getPasskeyCredential",
        "\n    at ",
    ] {
        if let Some(index) = message.find(marker) {
            message = message[..index].trim().to_owned();
        }
    }
    // Deduplicate doubled "Error: … Error: …" payloads.
    if let Some((first, _)) = message.split_once(" Error: ") {
        message = first.trim().to_owned();
    }

    let lower = message.to_ascii_lowercase();
    if lower.contains("notallowederror")
        || lower.contains("aborterror")
        || lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("timed out")
        || lower.contains("the operation either timed out")
        || message == "PASSKEY_CANCELLED"
    {
        return "Passkey prompt was cancelled.".to_owned();
    }
    if message.starts_with("JsValue(") {
        return "Passkey prompt was cancelled or unavailable.".to_owned();
    }
    message
}

pub(crate) fn is_passkey_cancel_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("cancelled")
        || lower.contains("canceled")
        || lower.contains("passkey_cancelled")
        || lower.contains("passkey_conditional_idle")
}

pub(crate) fn next_url() -> String {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
        {
            let query = search.trim_start_matches('?');
            if let Some(encoded) = query.split('&').find_map(|part| part.strip_prefix("next=")) {
                let value = percent_decode_component(encoded);
                if value.starts_with('/')
                    && !value.starts_with("//")
                    && !value.starts_with("/login")
                {
                    return value;
                }
            }
        }
    }
    "/dashboard".to_string()
}

pub(crate) fn first_http_url_in_text(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(url) = line
            .split_whitespace()
            .find(|part| part.starts_with("http://") || part.starts_with("https://"))
        {
            return Some(url.to_owned());
        }
    }
    None
}

pub(crate) fn percent_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

// Used from hydrate-only branches of next_url; keep available on SSR builds.
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
pub(crate) fn percent_decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
                {
                    out.push((high << 4) | low);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
pub(crate) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn one_time_token_from_url() -> Option<String> {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window()
            && let Ok(search) = window.location().search()
        {
            return search
                .trim_start_matches('?')
                .split('&')
                .find_map(|part| part.strip_prefix("token="))
                .map(ToOwned::to_owned)
                .filter(|value| !value.trim().is_empty());
        }
    }
    None
}

pub(crate) fn redirect_browser(url: &str) {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = window() {
            let location = window.location();
            if location.replace(url).is_err() {
                let _ = location.set_href(url);
            }
        }
    }
    let _ = url;
}

#[cfg_attr(feature = "hydrate", allow(dead_code))]
pub(crate) fn set_page_status(status: http::StatusCode) {
    #[cfg(feature = "ssr")]
    {
        if let Some(resp) = use_context::<leptos_wasi::response::ResponseOptions>() {
            resp.set_status(status);
        }
    }
    let _ = status;
}
