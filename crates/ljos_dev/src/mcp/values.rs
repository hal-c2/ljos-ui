//! A property's value as a person reads it.
//!
//! A script turns a list of objects into `[object Object],[object Object]`
//! and an object into `QQuickItem(0x…)`-like text, which tells nothing. A
//! list of objects is told by how many it holds, and each object by its
//! type and a handle any tool takes, so a click goes on to it.

use ljos_engine::{Engine, Value as QmlValue};
use serde_json::{Value, json};

use super::objects::handle;

/// How many objects of a list are named by handle.
const MAX_ITEMS: usize = 50;

/// What a value is shown as, and the handles of the objects it holds.
pub(crate) struct Shown {
    pub(crate) text: String,
    /// The object itself, for a value that is one.
    pub(crate) handle: Option<String>,
    /// The objects of a list, for a list that holds objects.
    pub(crate) items: Option<Vec<Value>>,
}

fn object_text(engine: &Engine, obj: ljos_engine::ObjectId) -> String {
    let ty = engine
        .meta_of(obj)
        .map_or_else(|| "object".to_owned(), |m| m.name.to_string());
    format!("{ty} {}", handle(obj))
}

pub(crate) fn shown(engine: &Engine, v: &QmlValue) -> Shown {
    match v {
        QmlValue::Object(obj) if engine.is_alive(*obj) => Shown {
            text: object_text(engine, *obj),
            handle: Some(handle(*obj)),
            items: None,
        },
        QmlValue::List(list) if list.iter().any(|e| matches!(e, QmlValue::Object(_))) => {
            let n = list.len();
            let items = list
                .iter()
                .filter_map(|e| match e {
                    QmlValue::Object(o) if engine.is_alive(*o) => Some(json!({
                        "handle": handle(*o),
                        "type": engine.meta_of(*o).map(|m| m.name.to_string()),
                    })),
                    _ => None,
                })
                .take(MAX_ITEMS)
                .collect();
            Shown {
                text: format!("{n} object{}", if n == 1 { "" } else { "s" }),
                handle: None,
                items: Some(items),
            }
        }
        QmlValue::List(list) if list.is_empty() => Shown {
            text: "0 objects".to_owned(),
            handle: None,
            items: Some(Vec::new()),
        },
        QmlValue::Map(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, f)| format!("{k}: {}", shown(engine, f).text))
                .collect();
            Shown {
                text: format!("{{{}}}", inner.join(", ")),
                handle: None,
                items: None,
            }
        }
        other => {
            // A script's own object prints as a script would print it.
            let text = engine
                .js_text(other)
                .unwrap_or_else(|| engine.to_js_string(other).to_string());
            Shown {
                text: if text == "[object Object]" {
                    "a script object".to_owned()
                } else {
                    text
                },
                handle: None,
                items: None,
            }
        }
    }
}
