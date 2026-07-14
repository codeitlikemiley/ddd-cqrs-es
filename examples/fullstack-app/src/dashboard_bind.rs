//! Shared query → widget projection (available on SSR and hydrate).

use crate::contracts::WidgetBind;
use serde_json::Value;

/// Extract JSON by simple dot path (`a.b.0.c`). Empty path returns root.
pub fn json_path_get(value: &Value, path: &str) -> Option<Value> {
    let path = path.trim();
    if path.is_empty() || path == "$" {
        return Some(value.clone());
    }
    let mut current = value;
    for segment in path
        .trim_start_matches("$.")
        .trim_start_matches('.')
        .split('.')
    {
        if segment.is_empty() {
            continue;
        }
        if let Ok(index) = segment.parse::<usize>() {
            current = current.as_array()?.get(index)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current.clone())
}

pub fn json_value_display(value: &Value) -> String {
    match value {
        Value::Null => "—".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(a) => format!("{} items", a.len()),
        Value::Object(o) => {
            if let Some(v) = o
                .get("value")
                .or_else(|| o.get("count"))
                .or_else(|| o.get("total"))
                .or_else(|| o.values().next())
            {
                json_value_display(v)
            } else {
                "{}".to_owned()
            }
        }
    }
}

fn bind_root_owned(data: &Value, bind: &WidgetBind) -> Value {
    if let Some(path) = bind
        .items_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        json_path_get(data, path).unwrap_or_else(|| data.clone())
    } else {
        data.clone()
    }
}

pub fn project_bound_metric(data_json: &str, bind: &WidgetBind) -> (String, String, String) {
    let parsed: Value = serde_json::from_str(data_json).unwrap_or(Value::Null);
    let root = bind_root_owned(&parsed, bind);
    let value =
        if let Some(path) = bind.value_path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            json_path_get(&root, path).unwrap_or(root.clone())
        } else {
            root.clone()
        };
    let label = bind
        .label_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|p| json_path_get(&root, p))
        .map(|v| json_value_display(&v))
        .unwrap_or_else(|| "Metric".to_owned());
    let meta = bind
        .meta_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(|p| json_path_get(&root, p))
        .map(|v| json_value_display(&v))
        .unwrap_or_default();
    (json_value_display(&value), label, meta)
}

pub fn project_bound_list(
    data_json: &str,
    bind: &WidgetBind,
    limit: usize,
) -> Vec<(String, String, String)> {
    let parsed: Value = serde_json::from_str(data_json).unwrap_or(Value::Null);
    let root = bind_root_owned(&parsed, bind);
    let rows: Vec<Value> = match root {
        Value::Array(items) => items,
        other => vec![other],
    };
    let title_path = bind.title_path.as_deref().unwrap_or("name").trim();
    let subtitle_path = bind.subtitle_path.as_deref().unwrap_or("").trim();
    let meta_path = bind.meta_path.as_deref().unwrap_or("").trim();
    rows.into_iter()
        .take(limit)
        .map(|row| {
            let title = if title_path.is_empty() {
                json_value_display(&row)
            } else {
                json_path_get(&row, title_path)
                    .map(|v| json_value_display(&v))
                    .unwrap_or_else(|| json_value_display(&row))
            };
            let subtitle = if subtitle_path.is_empty() {
                String::new()
            } else {
                json_path_get(&row, subtitle_path)
                    .map(|v| json_value_display(&v))
                    .unwrap_or_default()
            };
            let meta = if meta_path.is_empty() {
                String::new()
            } else {
                json_path_get(&row, meta_path)
                    .map(|v| json_value_display(&v))
                    .unwrap_or_default()
            };
            (title, subtitle, meta)
        })
        .collect()
}

pub fn project_bound_table(
    data_json: &str,
    bind: &WidgetBind,
    limit: usize,
) -> (Vec<String>, Vec<Vec<String>>) {
    let parsed: Value = serde_json::from_str(data_json).unwrap_or(Value::Null);
    let root = bind_root_owned(&parsed, bind);
    let rows: Vec<Value> = match root {
        Value::Array(items) => items,
        other => vec![other],
    };
    let columns: Vec<(String, String)> = if bind.columns.is_empty() {
        let keys = rows
            .first()
            .and_then(|r| r.as_object())
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        keys.into_iter().map(|k| (k.clone(), k)).collect()
    } else {
        bind.columns.clone()
    };
    let headers: Vec<String> = columns.iter().map(|(_, h)| h.clone()).collect();
    let body: Vec<Vec<String>> = rows
        .into_iter()
        .take(limit)
        .map(|row| {
            columns
                .iter()
                .map(|(key, _)| {
                    json_path_get(&row, key)
                        .map(|v| json_value_display(&v))
                        .unwrap_or_else(|| "—".to_owned())
                })
                .collect()
        })
        .collect();
    (headers, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::WidgetBind;

    #[test]
    fn project_bound_metric_and_list() {
        let json = r#"{"count":42,"label":"users"}"#;
        let bind = WidgetBind {
            value_path: Some("count".into()),
            label_path: Some("label".into()),
            ..Default::default()
        };
        let (v, l, _) = project_bound_metric(json, &bind);
        assert_eq!(v, "42");
        assert_eq!(l, "users");

        let list_json = r#"[{"name":"a","id":"1"},{"name":"b","id":"2"}]"#;
        let list_bind = WidgetBind {
            title_path: Some("name".into()),
            subtitle_path: Some("id".into()),
            ..Default::default()
        };
        let rows = project_bound_list(list_json, &list_bind, 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "a");
    }
}
