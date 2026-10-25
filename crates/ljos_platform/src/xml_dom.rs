//! The document `XMLHttpRequest.responseXML` gives, read out of a reply:
//! the read-only part of the W3C DOM Qt has
//! (doc.qt.io/qt-6/qtqml-javascript-qmlglobalobject.html).
//!
//! The reply is read here into a tree the script's side turns into nodes
//! (`xhr.rs`), and only when a script asks for it. As Qt's, the tree has
//! elements, text and CDATA sections, and no comments or processing
//! instructions; names are local names, and a namespace declaration is no
//! attribute. A reply that is not well-formed XML has no document.

use quick_xml::{
    Reader, XmlVersion,
    escape::resolve_predefined_entity,
    events::{BytesStart, Event},
};
use serde_json::{Value as Json, json};

/// Whether a reply of this `Content-Type` is a document: an XML type, as
/// the XHR standard says (`text/xml`, `application/xml`, `…+xml`).
pub(crate) fn is_xml(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or("").trim();
    let essence = essence.to_ascii_lowercase();
    essence == "text/xml" || essence == "application/xml" || essence.ends_with("+xml")
}

/// The document in `text`, as JSON: `{version, encoding, standalone,
/// root}`, where an element is `[1, name, [[name, value]…], [child…]]`,
/// text `[3, data]` and a CDATA section `[4, data]`. `None` when `text` is
/// not well-formed XML.
pub(crate) fn document(text: &str) -> Option<String> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let (mut version, mut encoding, mut standalone) = ("1.0".to_owned(), String::new(), false);
    // The open elements, each with what it has so far.
    let mut open: Vec<(String, Json, Vec<Json>)> = Vec::new();
    let mut root: Option<Json> = None;
    // Text and references next to each other are one text node.
    let mut pending = String::new();
    loop {
        let event = reader.read_event().ok()?;
        if !matches!(event, Event::Text(_) | Event::GeneralRef(_)) {
            flush(&mut pending, &mut open)?;
        }
        match event {
            Event::Decl(d) => {
                version = d.version().ok()?.into_owned();
                if let Some(e) = d.encoding() {
                    encoding = e.ok()?.into_owned();
                }
                standalone = d.standalone().is_some_and(|s| s.is_ok_and(|s| s == "yes"));
            }
            Event::Start(e) => {
                if root.is_some() {
                    return None;
                }
                open.push((local(e.name().as_ref()), attributes(&e)?, Vec::new()));
            }
            Event::Empty(e) => {
                let element = json!([1, local(e.name().as_ref()), attributes(&e)?, []]);
                close(element, &mut open, &mut root)?;
            }
            Event::End(_) => {
                let (name, attributes, children) = open.pop()?;
                close(json!([1, name, attributes, children]), &mut open, &mut root)?;
            }
            Event::Text(t) => pending.push_str(&t.xml10_content()),
            Event::GeneralRef(r) => {
                let c = r.resolve_char_ref().ok()?;
                match c {
                    Some(c) => pending.push(c),
                    None => pending.push_str(resolve_predefined_entity(&r.xml10_content())?),
                }
            }
            Event::CData(t) => match open.last_mut() {
                Some((_, _, children)) => children.push(json!([4, t.xml10_content()])),
                None => return None,
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if !open.is_empty() {
        return None;
    }
    let doc = json!({
        "version": version, "encoding": encoding, "standalone": standalone, "root": root?,
    });
    Some(doc.to_string())
}

/// Puts the text gathered so far in the open element. Outside every
/// element only white space may be.
fn flush(pending: &mut String, open: &mut [(String, Json, Vec<Json>)]) -> Option<()> {
    if pending.is_empty() {
        return Some(());
    }
    let text = std::mem::take(pending);
    match open.last_mut() {
        Some((_, _, children)) => children.push(json!([3, text])),
        None if text.trim().is_empty() => {}
        None => return None,
    }
    Some(())
}

/// An element that ended goes in the one around it, or is the root.
fn close(
    element: Json,
    open: &mut [(String, Json, Vec<Json>)],
    root: &mut Option<Json>,
) -> Option<()> {
    match open.last_mut() {
        Some((_, _, children)) => children.push(element),
        None if root.is_none() => *root = Some(element),
        None => return None,
    }
    Some(())
}

fn attributes(e: &BytesStart<'_>) -> Option<Json> {
    let mut out = Vec::new();
    for attr in e.attributes() {
        let attr = attr.ok()?;
        let key: &str = attr.key.as_ref();
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let value = attr.normalized_value(XmlVersion::Implicit1_0).ok()?;
        out.push(json!([local(key), value]));
    }
    Some(Json::Array(out))
}

fn local(qname: &str) -> String {
    match qname.rsplit_once(':') {
        Some((_, local)) => local.to_owned(),
        None => qname.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(text: &str) -> Json {
        serde_json::from_str(&document(text).expect("a document")).unwrap()
    }

    #[test]
    fn an_xml_type_is_a_document_and_another_is_not() {
        assert!(is_xml("text/xml"));
        assert!(is_xml("application/xml; charset=utf-8"));
        assert!(is_xml("application/atom+xml"));
        assert!(!is_xml("text/plain"));
        assert!(!is_xml("application/json"));
        assert!(!is_xml(""));
    }

    #[test]
    fn text_and_references_beside_it_are_one_text_node() {
        let doc = tree("<a>x &amp; &#65;<b/>y</a>");
        assert_eq!(
            doc["root"],
            json!([1, "a", [], [[3, "x & A"], [1, "b", [], []], [3, "y"]]])
        );
    }

    #[test]
    fn names_are_local_and_a_namespace_declaration_is_no_attribute() {
        let doc = tree(r#"<x:r xmlns:x="urn:x" xmlns="urn:d" x:a="1 &lt; 2"/>"#);
        assert_eq!(doc["root"], json!([1, "r", [["a", "1 < 2"]], []]));
    }

    #[test]
    fn the_declaration_says_version_encoding_and_standalone() {
        let doc = tree(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a/>"#);
        assert_eq!(
            (&doc["version"], &doc["encoding"], &doc["standalone"]),
            (&json!("1.0"), &json!("UTF-8"), &json!(true))
        );
        let bare = tree("<a/>");
        assert_eq!(
            (&bare["encoding"], &bare["standalone"]),
            (&json!(""), &json!(false))
        );
    }

    #[test]
    fn comments_and_processing_instructions_are_left_out_and_cdata_is_kept() {
        let doc = tree("<!-- c --><a><!-- d --><?pi x?><![CDATA[x<y]]></a>\n");
        assert_eq!(doc["root"], json!([1, "a", [], [[4, "x<y"]]]));
    }

    #[test]
    fn what_is_not_well_formed_has_no_document() {
        for text in [
            "<a><b></a>",
            "<a>",
            "",
            "not xml",
            "<a/><b/>",
            "<a/>x",
            "<a>&nope;</a>",
        ] {
            assert_eq!(document(text), None, "{text:?}");
        }
    }
}
