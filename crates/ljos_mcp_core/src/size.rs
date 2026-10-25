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
/// Shrinks what holds most of the bytes, however deep: a list whose bytes
/// are spread over its elements is halved, one whose single element holds
/// most of them keeps that element and shrinks inside it, and a long string
/// is cut. Sets `truncated: true`. A top-level list that is cut also gets
/// `next_cursor` (the index of its first dropped element, as a string) and
/// `truncated_field`, so the caller can resume; a tool that paginates itself
/// and already set `next_cursor` keeps its own cursor. A cut deeper down has
/// no cursor to resume from. Non-object values are wrapped as `{"value": …}`
/// because structured results are always objects.
pub fn cap_result(value: Value, max_bytes: usize) -> Value {
    let max_bytes = max_bytes.min(HARD_RESULT_BYTES);
    let obj = match value {
        Value::Object(o) => o,
        other => Map::from_iter([("value".to_owned(), other)]),
    };
    let mut value = Value::Object(obj);
    if size_of(&value) <= max_bytes {
        return value;
    }
    let Value::Object(obj) = &mut value else {
        unreachable!("wrapped above")
    };
    obj.insert("truncated".into(), Value::Bool(true));
    let own_cursor = obj.contains_key("next_cursor");

    while size_of(&value) > max_bytes {
        let Some(cut) = shrink(&mut value, true) else {
            break;
        };
        if let (Some((key, keep)), false) = (cut, own_cursor)
            && let Value::Object(obj) = &mut value
        {
            obj.insert("next_cursor".into(), json!(keep.to_string()));
            obj.insert("truncated_field".into(), json!(key));
        }
    }
    value
}

/// Shrinks the part of `value` that holds most of its bytes, once.
/// `None`: nothing is left to shrink. `Some(Some((key, kept)))`: a list of
/// the top-level object (`top`) was halved to `kept` elements.
fn shrink(value: &mut Value, top: bool) -> Option<Option<(String, usize)>> {
    match value {
        Value::String(s) if s.len() > 64 => {
            let mut cut = s.len() / 2;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            s.truncate(cut);
            Some(None)
        }
        Value::Object(map) => {
            // Biggest first; a child with nothing to shrink gives way to
            // the next one.
            let mut keys: Vec<(String, usize)> =
                map.iter().map(|(k, v)| (k.clone(), size_of(v))).collect();
            keys.sort_by_key(|(_, s)| std::cmp::Reverse(*s));
            for (key, _) in keys {
                let child = map.get_mut(&key)?;
                let was_list = matches!(child, Value::Array(a) if !a.is_empty());
                let before = child.as_array().map(Vec::len);
                if let Some(cut) = shrink(child, false) {
                    let after = child.as_array().map(Vec::len);
                    let halved = was_list && before != after;
                    return Some(match (top && halved, after) {
                        (true, Some(kept)) => Some((key, kept)),
                        _ => cut,
                    });
                }
            }
            None
        }
        Value::Array(items) if !items.is_empty() => {
            let sizes: Vec<usize> = items.iter().map(size_of).collect();
            let total: usize = sizes.iter().sum();
            let (big, &most) = sizes
                .iter()
                .enumerate()
                .max_by_key(|(_, s)| **s)
                .unwrap_or((0, &0));
            // One element that holds most of the list is shrunk rather
            // than dropped with the rest.
            if most * 2 > total && shrink(&mut items[big], false).is_some() {
                return Some(None);
            }
            let keep = items.len() / 2;
            items.truncate(keep);
            Some(None)
        }
        _ => None,
    }
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
    fn nested_lists_are_shrunk_too() {
        let types: Vec<Value> = (0..5000)
            .map(|i| json!({ "type": format!("Type{i:05}"), "count": i, "bytes": i * 64 }))
            .collect();
        let out = cap_result(
            json!({ "objects": { "count": 5000, "types": types }, "next_since": 7 }),
            4096,
        );
        assert!(size_of(&out) <= 4096);
        assert_eq!(out["truncated"], true);
        assert!(!out["objects"]["types"].as_array().unwrap().is_empty());
        // What the tool says to resume from is kept as it was, and a cut
        // deep down has no cursor of its own.
        assert_eq!(out["next_since"], 7);
        assert_eq!(out["objects"]["count"], 5000);
        assert!(out.get("next_cursor").is_none());
    }

    #[test]
    fn one_element_that_holds_most_is_shrunk_and_kept() {
        let out = cap_result(
            json!({ "events": [ { "seq": 1, "value": "x".repeat(100_000) } ], "next_since": 1 }),
            2048,
        );
        assert!(size_of(&out) <= 2048);
        assert_eq!(out["events"][0]["seq"], 1);
        assert!(out.get("next_cursor").is_none());
    }

    #[test]
    fn deep_values_are_shrunk() {
        let deep = json!({ "a": [[[["y".repeat(50_000), "z".repeat(50_000)]]]] });
        let out = cap_result(deep, 1024);
        assert!(size_of(&out) <= 1024);
    }

    #[test]
    fn a_tool_cursor_is_kept_when_a_top_list_is_cut() {
        let items: Vec<String> = (0..5000).map(|i| format!("item-{i:05}")).collect();
        let out = cap_result(json!({ "items": items, "next_cursor": "own" }), 4096);
        assert!(size_of(&out) <= 4096);
        assert_eq!(out["next_cursor"], "own");
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
