//! JSON read into rows. This runs on a worker for a file, so nothing here
//! touches the engine's values, which stay on its thread.

use std::fmt;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

/// A document larger than this is refused, rows kept: a model holds all of
/// its rows in memory, so the limit is on what a stray file can cost.
pub const DOCUMENT_BYTES: usize = 64 * 1024 * 1024;

/// One JSON value, keys in document order.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Number(f64),
    Text(Box<str>),
    List(Vec<Node>),
    Map(Vec<(Box<str>, Node)>),
}

/// One object of the array.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub fields: Vec<(Box<str>, Node)>,
}

impl Row {
    pub fn field(&self, key: &str) -> Option<&Node> {
        self.fields
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v)
    }

    /// The same keys with the same values, in whichever order: the order of
    /// the keys of an object is no data of a row.
    pub fn same(&self, other: &Row) -> bool {
        self.fields == other.fields
            || (self.fields.len() == other.fields.len()
                && self
                    .fields
                    .iter()
                    .all(|(k, v)| other.field(k).is_some_and(|o| o == v)))
    }
}

/// Where a document went wrong; the caller knows what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// Line and column in the text, when it is the JSON that is broken.
    pub at: Option<(usize, usize)>,
    pub message: String,
}

impl Fault {
    fn of(message: impl Into<String>) -> Self {
        Fault {
            at: None,
            message: message.into(),
        }
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Node, E> {
        Ok(Node::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Node, E> {
        Ok(Node::Number(v as f64))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Node, E> {
        Ok(Node::Number(v as f64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Node, E> {
        Ok(Node::Number(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Node, E> {
        Ok(Node::Text(v.into()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element::<Node>()? {
            items.push(item);
        }
        Ok(Node::List(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut fields: Vec<(Box<str>, Node)> = Vec::new();
        while let Some((key, value)) = map.next_entry::<Box<str>, Node>()? {
            // As JSON.parse: the last value, where the key was first said.
            match fields.iter_mut().find(|(k, _)| *k == key) {
                Some(field) => field.1 = value,
                None => fields.push((key, value)),
            }
        }
        Ok(Node::Map(fields))
    }
}

impl<'de> de::Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        d.deserialize_any(NodeVisitor)
    }
}

/// The rows of a document: an array of objects.
pub fn rows(bytes: &[u8]) -> Result<Vec<Row>, Fault> {
    if bytes.len() > DOCUMENT_BYTES {
        return Err(Fault::of(format!(
            "the document is larger than {DOCUMENT_BYTES} bytes"
        )));
    }
    // A byte order mark is not JSON, but editors write it.
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let node: Node = serde_json::from_slice(bytes).map_err(|err| Fault {
        at: Some((err.line(), err.column())),
        message: err.to_string(),
    })?;
    let Node::List(items) = node else {
        return Err(Fault::of("the document is not an array of objects"));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| match item {
            Node::Map(fields) => Ok(Row { fields }),
            _ => Err(Fault::of(format!("row {i} is not an object"))),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_document_says_where() {
        let fault = rows(b"[{\"a\": 1},\n {\"a\": }]").unwrap_err();
        assert_eq!(fault.at.map(|(line, _)| line), Some(2));
    }

    #[test]
    fn nesting_past_the_parser_limit_is_a_fault_not_a_crash() {
        let deep = "[".repeat(200_000);
        assert!(rows(deep.as_bytes()).is_err());
    }

    #[test]
    fn a_document_over_the_limit_is_refused_unread() {
        let big = vec![b' '; DOCUMENT_BYTES + 1];
        assert!(rows(&big).unwrap_err().message.contains("larger"));
    }

    #[test]
    fn a_byte_order_mark_is_passed_over() {
        assert_eq!(rows(b"\xef\xbb\xbf[]").unwrap().len(), 0);
    }

    #[test]
    fn the_same_row_may_say_its_keys_in_another_order() {
        let got = rows(br#"[{"a":1,"b":2},{"b":2,"a":1},{"a":1,"b":3},{"a":1}]"#).unwrap();
        assert!(got[0].same(&got[1]));
        assert!(!got[0].same(&got[2]));
        assert!(!got[0].same(&got[3]));
    }
}
