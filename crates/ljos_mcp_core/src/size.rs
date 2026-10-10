use serde_json::{Map, Value, json};

/// Default response budget (§19.6): well under where clients drop results.
pub const DEFAULT_RESULT_BYTES: usize = 40 * 1024;
/// No tool may raise its own budget above this.
pub const HARD_RESULT_BYTES: usize = 80 * 1024;

fn size_of(value: &Value) -> usize {
    // Serialising a `Value` cannot fail (string keys only).
    serde_json::to_vec(value).map_or(0, |v| v.len())
}

/// Brings `value` under `max_bytes` of compact JSON.
///
/// Shrinks the largest top-level array first, then the largest string, and sets
/// `truncated: true` plus `next_cursor` (the index of the first dropped array
/// element, as a string) so the caller can resume. A tool that paginates
/// itself and already set `next_cursor` keeps its own cursor. Non-object values
/// are wrapped as `{"value": …}` because structured results are always objects.
pub fn cap_result(value: Value, max_bytes: usize) -> Value {
    let max_bytes = max_bytes.min(HARD_RESULT_BYTES);
    let mut obj = match value {
        Value::Object(o) => o,
        other => Map::from_iter([("value".to_owned(), other)]),
    };
    if size_of(&Value::Object(obj.clone())) <= max_bytes {
        return Value::Object(obj);
    }
    obj.insert("truncated".into(), Value::Bool(true));
    let own_cursor = obj.contains_key("next_cursor");

    while size_of(&Value::Object(obj.clone())) > max_bytes {
        if shrink_array(&mut obj, own_cursor) || shrink_string(&mut obj) {
            continue;
        }
        break;
    }
    Value::Object(obj)
}

/// Halves the largest array. Halving keeps this O(n log n) for huge lists.
fn shrink_array(obj: &mut Map<String, Value>, own_cursor: bool) -> bool {
    let Some(key) = obj
        .iter()
        .filter_map(|(k, v)| match v {
            Value::Array(a) if !a.is_empty() => Some((k.clone(), size_of(v))),
            _ => None,
        })
        .max_by_key(|(_, s)| *s)
        .map(|(k, _)| k)
    else {
        return false;
    };
    let Some(Value::Array(items)) = obj.get_mut(&key) else {
        return false;
    };
    let keep = items.len() / 2;
    items.truncate(keep);
    if !own_cursor {
        obj.insert("next_cursor".into(), json!(keep.to_string()));
        obj.insert("truncated_field".into(), json!(key));
    }
    true
}

fn shrink_string(obj: &mut Map<String, Value>) -> bool {
    let Some(key) = obj
        .iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) if s.len() > 64 => Some((k.clone(), s.len())),
            _ => None,
        })
        .max_by_key(|(_, l)| *l)
        .map(|(k, _)| k)
    else {
        return false;
    };
    if let Some(Value::String(s)) = obj.get_mut(&key) {
        let mut cut = s.len() / 2;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    true
}

/// Slices `items` for a cursor-paginated tool. `cursor` is the start index as
/// a string (the same format [`cap_result`] emits); returns the page and the
/// next cursor, if any.
pub fn paginate<T>(
    items: Vec<T>,
    cursor: Option<&str>,
    limit: usize,
) -> Result<(Vec<T>, Option<String>), crate::SurfaceError> {
    let start = match cursor {
        None => 0,
        Some(c) => c
            .parse::<usize>()
            .map_err(|_| crate::SurfaceError::invalid_argument(format!("bad cursor `{c}`")))?,
    };
    let total = items.len();
    let end = start.saturating_add(limit).min(total);
    let page: Vec<T> = items.into_iter().skip(start).take(limit).collect();
    Ok((page, (end < total).then(|| end.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_pass_through_untouched() {
        let v = json!({"items": [1, 2, 3]});
        assert_eq!(cap_result(v.clone(), DEFAULT_RESULT_BYTES), v);
    }

    #[test]
    fn large_arrays_are_truncated_with_cursor() {
        let items: Vec<String> = (0..5000).map(|i| format!("item-{i:05}")).collect();
        let out = cap_result(json!({ "items": items }), 4096);
        assert!(size_of(&out) <= 4096);
        assert_eq!(out["truncated"], true);
        let kept = out["items"].as_array().unwrap().len();
        assert_eq!(out["next_cursor"], kept.to_string());
        assert_eq!(out["truncated_field"], "items");
    }

    #[test]
    fn large_strings_are_cut_on_a_char_boundary() {
        let out = cap_result(json!({ "text": "é".repeat(10_000) }), 1024);
        assert!(size_of(&out) <= 1024);
        assert_eq!(out["truncated"], true);
    }

    #[test]
    fn tool_budget_cannot_exceed_the_hard_cap() {
        let out = cap_result(json!({ "text": "x".repeat(200_000) }), usize::MAX);
        assert!(size_of(&out) <= HARD_RESULT_BYTES);
    }

    #[test]
    fn non_objects_are_wrapped() {
        assert_eq!(cap_result(json!(3), 100), json!({"value": 3}));
    }

    #[test]
    fn paginate_walks_all_items() {
        let (page, next) = paginate((0..5).collect(), None, 2).unwrap();
        assert_eq!((page, next.as_deref()), (vec![0, 1], Some("2")));
        let (page, next) = paginate((0..5).collect(), Some("4"), 2).unwrap();
        assert_eq!((page, next), (vec![4], None));
        assert!(paginate(vec![1], Some("x"), 2).is_err());
    }
}
