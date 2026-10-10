//! Reading and comparing the values that steps write in Gherkin.

use serde_json::Value;

/// Parse a step argument: `5`, `0.5`, `true`, `null`, `"text"`, `[1, 2]`.
///
/// # Errors
/// When the text is not JSON; the message tells the author how to fix it.
pub fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text.trim()).map_err(|_| {
        format!("cannot read {text:?} as a value; write a number, true, false, null, or a \"quoted string\"")
    })
}

/// Parse a comma separated argument list (`1, "a", true`) for `call`.
///
/// # Errors
/// When any element is not a value.
pub fn parse_list(text: &str) -> Result<Vec<Value>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str::<Value>(&format!("[{text}]")) {
        Ok(Value::Array(v)) => Ok(v),
        _ => Err(format!("cannot read {text:?} as a list of values")),
    }
}

/// Equality as a QML author means it: numbers compare as numbers (so `6` equals
/// `6.0`), colours as case-insensitive hex.
#[must_use]
pub fn same(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-9,
            _ => a == b,
        },
        (Value::String(a), Value::String(b)) => {
            a.eq_ignore_ascii_case(b) && (a.starts_with('#') || a == b) || a == b
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        _ => expected == actual,
    }
}

/// `actual` as a number, for the ordering steps.
#[must_use]
pub fn number(actual: &Value) -> Option<f64> {
    actual.as_f64()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_values() {
        assert_eq!(parse(" 5 ").unwrap(), json!(5));
        assert_eq!(parse("\"a b\"").unwrap(), json!("a b"));
        assert!(parse("red").unwrap_err().contains("quoted"));
    }

    #[test]
    fn parses_lists() {
        assert_eq!(
            parse_list("1, \"a\", true").unwrap(),
            vec![json!(1), json!("a"), json!(true)]
        );
        assert!(parse_list("").unwrap().is_empty());
        assert!(parse_list("1,,").is_err());
    }

    #[test]
    fn numbers_compare_as_numbers() {
        assert!(same(&json!(6), &json!(6.0)));
        assert!(!same(&json!(6), &json!(7)));
    }

    #[test]
    fn hex_colours_ignore_case_but_text_does_not() {
        assert!(same(&json!("#FF0000"), &json!("#ff0000")));
        assert!(!same(&json!("Red"), &json!("red")));
    }
}
