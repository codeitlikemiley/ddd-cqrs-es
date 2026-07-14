//! Small dashboard board helpers.

#![allow(unused_imports)]

use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;


pub(crate) fn event_target_value_board(event: &leptos::ev::Event) -> String {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast;
        return event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            .map(|el| el.value())
            .or_else(|| {
                event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
                    .map(|el| el.value())
            })
            .unwrap_or_default();
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = event;
        String::new()
    }
}

pub(crate) fn parse_list_labels(data_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(data_json) else {
        return vec![data_json.to_owned()];
    };
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(map) => map
                    .get("name")
                    .or_else(|| map.get("title"))
                    .or_else(|| map.get("id"))
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_else(|| "{}".to_owned()),
                other => other.to_string(),
            })
            .collect(),
        serde_json::Value::String(s) => vec![s],
        other => vec![other.to_string()],
    }
}

pub(crate) fn relative_ms(ms: u64) -> String {
    let now_ms = current_unix_ms();
    if ms > now_ms {
        let delta = ms - now_ms;
        if delta < 60_000 {
            return "soon".to_owned();
        }
        if delta < 3_600_000 {
            return format!("in {}m", delta / 60_000);
        }
        return format!("in {}h", delta / 3_600_000);
    }
    let delta = now_ms.saturating_sub(ms);
    if delta < 60_000 {
        return "just now".to_owned();
    }
    if delta < 3_600_000 {
        return format!("{}m ago", delta / 60_000);
    }
    if delta < 86_400_000 {
        return format!("{}h ago", delta / 3_600_000);
    }
    format!("{}d ago", delta / 86_400_000)
}

pub(crate) fn current_unix_ms() -> u64 {
    #[cfg(feature = "hydrate")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(feature = "hydrate"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
