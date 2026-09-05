// -------------------------------------------------------------------------
// Postgres formatting & local query interpolation helpers
// -------------------------------------------------------------------------
/// Convert a JSON value into a SQL literal for PostgreSQL text interpolation.
///
/// # Safety contract
///
/// The returned literal is safe to embed only while the server runs with
/// `standard_conforming_strings = on` (the default since PostgreSQL 9.1):
/// single quotes are doubled and backslashes are treated as ordinary
/// characters. Under legacy `E''` escape-string syntax, backslash sequences
/// inside values would become escape characters, so untrusted input must be
/// bound through a parameterized transport instead.
pub fn format_pg_value(val: &serde_json::Value) -> Result<String, String> {
    match val {
        serde_json::Value::Null => Ok("NULL".to_string()),
        serde_json::Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('\'', "''");
            Ok(format!("'{}'", escaped))
        }
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            let s = serde_json::to_string(val).map_err(|e| e.to_string())?;
            let escaped = s.replace('\\', "\\\\").replace('\'', "''");
            Ok(format!("'{}'", escaped))
        }
    }
}

/// Replace `$1`, `$2`, ... placeholders in a SQL template with interpolated literals.
///
/// Prefer server-side `query_params` (Supabase RPC, Neon HTTP) whenever available.
/// When interpolation is unavoidable, values are rendered by [`format_pg_value`],
/// which doubles backslashes and single quotes. Callers must still verify
/// `standard_conforming_strings = on` on the target database.
pub fn interpolate_query(sql: &str, params: &[serde_json::Value]) -> Result<String, String> {
    let mut final_sql = String::new();
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            let mut digits = String::new();
            while let Some(&next_c) = chars.peek() {
                if next_c.is_ascii_digit() {
                    digits.push(next_c);
                    chars.next();
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                final_sql.push('$');
            } else {
                let idx = digits.parse::<usize>().map_err(|e| e.to_string())?;
                if idx == 0 || idx > params.len() {
                    return Err(format!(
                        "Parameter index ${} out of bounds (params len: {})",
                        idx,
                        params.len()
                    ));
                }
                let param_val = &params[idx - 1];
                let formatted = format_pg_value(param_val)?;
                final_sql.push_str(&formatted);
            }
        } else {
            final_sql.push(c);
        }
    }

    Ok(final_sql)
}

/// Base64-encode binary payloads without padding surprises.
pub fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as usize;
        let b1 = if i + 1 < input.len() {
            input[i + 1] as usize
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            input[i + 2] as usize
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARSET[(triple >> 18) & 63] as char);
        result.push(CHARSET[(triple >> 12) & 63] as char);
        result.push(if i + 1 < input.len() {
            CHARSET[(triple >> 6) & 63] as char
        } else {
            '='
        });
        result.push(if i + 2 < input.len() {
            CHARSET[triple & 63] as char
        } else {
            '='
        });

        i += 3;
    }
    result
}

#[cfg(test)]
mod pg_interpolation_tests {
    use super::{format_pg_value, interpolate_query};
    use serde_json::json;

    #[test]
    fn doubles_single_quotes_inside_strings() {
        assert_eq!(format_pg_value(&json!("O'Brien")).unwrap(), "'O''Brien'");
    }

    #[test]
    fn injection_attempt_stays_within_one_literal() {
        let value = json!("x'); DROP TABLE events; --");
        let formatted = format_pg_value(&value).unwrap();
        assert_eq!(formatted, "'x''); DROP TABLE events; --'");

        let sql = interpolate_query("SELECT $1", &[value]).unwrap();
        assert_eq!(sql, "SELECT 'x''); DROP TABLE events; --'");
    }

    #[test]
    fn backslashes_are_doubled_for_interpolation_safety() {
        let value = json!(r"\'; DELETE");
        assert_eq!(format_pg_value(&value).unwrap(), "'\\\\''; DELETE'");
    }

    #[test]
    fn formats_scalars() {
        assert_eq!(format_pg_value(&json!(null)).unwrap(), "NULL");
        assert_eq!(format_pg_value(&json!(true)).unwrap(), "true");
        assert_eq!(format_pg_value(&json!(false)).unwrap(), "false");
        assert_eq!(format_pg_value(&json!(42)).unwrap(), "42");
        assert_eq!(format_pg_value(&json!(-1.5)).unwrap(), "-1.5");
    }

    #[test]
    fn formats_json_containers_as_escaped_text() {
        assert_eq!(
            format_pg_value(&json!([1, "a'b"])).unwrap(),
            "'[1,\"a''b\"]'"
        );
        assert_eq!(
            format_pg_value(&json!({"k": "v'"})).unwrap(),
            "'{\"k\":\"v''\"}'"
        );
    }

    #[test]
    fn substitutes_numbered_placeholders_in_any_order() {
        let sql = "SELECT $2, $1";
        let interpolated = interpolate_query(sql, &[json!("one"), json!(2)]).unwrap();
        assert_eq!(interpolated, "SELECT 2, 'one'");
    }

    #[test]
    fn handles_multi_digit_and_adjacent_placeholders() {
        let mut params = vec![serde_json::Value::Null; 10];
        params[9] = json!("tenth");
        assert_eq!(interpolate_query("$10", &params).unwrap(), "'tenth'");
        assert!(interpolate_query("$11", &params).is_err());

        let adjacent = interpolate_query("$1$2", &[json!(1), json!(2)]).unwrap();
        assert_eq!(adjacent, "12");
    }

    #[test]
    fn placeholder_like_text_in_param_data_is_not_rescanned() {
        // The template's quoted `$1` placeholder is substituted once; the
        // inserted value's own `$1` text must not trigger another pass.
        let interpolated = interpolate_query("SELECT '$1'", &[json!("$1")]).unwrap();
        assert_eq!(interpolated, "SELECT ''$1''");
    }

    #[test]
    fn rejects_invalid_placeholder_indexes() {
        assert!(interpolate_query("SELECT $0", &[json!(1)]).is_err());
        assert!(interpolate_query("SELECT $3", &[json!(1), json!(2)]).is_err());
        assert_eq!(
            interpolate_query("SELECT $$", &[json!(1)]).unwrap(),
            "SELECT $$"
        );
    }
}
