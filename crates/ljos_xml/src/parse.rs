//! Reading rows out of an XML document, the way the Qt documentation
//! describes `XmlListModel` doing it: a slash-separated path picks the
//! elements that become rows, and each role reads the text (or one
//! attribute) of an element below a row's element.
//!
//! The reader is a stream, as Qt's is: a row exists from the moment its
//! element opens, and a document that breaks part-way keeps the rows
//! already read and reports a query error instead of failing the load.
//! Element and attribute names match by their local name, so a role
//! named `content` reads a `<media:content>` element.

use quick_xml::{
    Reader, XmlVersion,
    escape::resolve_predefined_entity,
    events::{BytesStart, Event},
};

/// One role to read out of each row element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSpec {
    /// The role name a delegate sees.
    pub name: String,
    /// The path below the row element (`info/pages`); empty is the row
    /// element itself.
    pub element: Vec<String>,
    /// The attribute to read instead of the element's text; empty
    /// reads the text.
    pub attribute: String,
}

/// What a document gave: one value per role, per row, and the warnings
/// reading it raised (already worded as Qt words them, without the
/// `QML XmlListModel:` prefix a log line adds).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

/// The path a query names: `/rss/channel/item` is the three names.
/// `None` when the query does not start with `/`, the one form Qt
/// refuses (with a warning) instead of matching nothing.
pub fn query_path(query: &str) -> Option<Vec<String>> {
    let rest = query.strip_prefix('/')?;
    Some(rest.split('/').map(str::to_owned).collect())
}

/// The path a role's `elementName` names, below the row element.
pub fn element_path(element_name: &str) -> Vec<String> {
    if element_name.is_empty() {
        Vec::new()
    } else {
        element_name.split('/').map(str::to_owned).collect()
    }
}

/// A row being read: the finished values, and per role the text being
/// gathered while its element is open.
struct Row {
    values: Vec<String>,
    gathering: Vec<Option<String>>,
}

/// Reads `bytes` as the document the model shows. Never fails: what
/// could be read is in `rows`, what went wrong is in `warnings`.
pub fn parse(bytes: &[u8], query: &[String], roles: &[RoleSpec]) -> Parsed {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    // The full path each role gathers at: the row's path, then its own.
    let targets: Vec<Vec<String>> = roles
        .iter()
        .map(|r| {
            let mut t = query.to_vec();
            t.extend(r.element.iter().cloned());
            t
        })
        .collect();
    let mut out = Parsed::default();
    let mut stack: Vec<String> = Vec::new();
    let mut row: Option<Row> = None;
    let mut saw_root = false;
    // The missing-attribute warning is said once per role, not per row.
    let mut warned: Vec<bool> = vec![false; roles.len()];
    let mut broken: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                saw_root = true;
                stack.push(local(e.name().as_ref()));
                start_element(
                    &e,
                    &stack,
                    query,
                    roles,
                    &targets,
                    &mut row,
                    &mut warned,
                    &mut out.warnings,
                );
            }
            Ok(Event::Empty(e)) => {
                saw_root = true;
                stack.push(local(e.name().as_ref()));
                start_element(
                    &e,
                    &stack,
                    query,
                    roles,
                    &targets,
                    &mut row,
                    &mut warned,
                    &mut out.warnings,
                );
                end_element(&stack, query, &targets, &mut row, &mut out.rows);
                stack.pop();
            }
            Ok(Event::End(_)) => {
                end_element(&stack, query, &targets, &mut row, &mut out.rows);
                stack.pop();
            }
            Ok(Event::Text(t)) => gather(&mut row, &stack, &targets, &t.xml10_content()),
            Ok(Event::CData(t)) => gather(&mut row, &stack, &targets, &t.xml10_content()),
            Ok(Event::GeneralRef(r)) => {
                let text = r.resolve_char_ref().ok().flatten().map(|c| c.to_string());
                let text = text
                    .or_else(|| resolve_predefined_entity(&r.xml10_content()).map(str::to_owned));
                match text {
                    Some(text) => gather(&mut row, &stack, &targets, &text),
                    None => {
                        broken = Some(format!("Unknown entity &{};.", r.xml10_content()));
                        break;
                    }
                }
            }
            Ok(Event::Eof) => {
                if !saw_root || !stack.is_empty() {
                    broken = Some("Premature end of document.".to_owned());
                }
                break;
            }
            Ok(_) => {}
            Err(e) => {
                broken = Some(e.to_string());
                break;
            }
        }
    }
    // A row whose element never closed was still read (Qt keeps it).
    if let Some(row) = row.take() {
        out.rows.push(finish(row));
    }
    if let Some(message) = broken {
        out.warnings.push(format!("Query error: \"{message}\""));
    }
    out
}

fn local(qname: &str) -> String {
    match qname.rsplit_once(':') {
        Some((_, local)) => local.to_owned(),
        None => qname.to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn start_element(
    e: &BytesStart<'_>,
    stack: &[String],
    query: &[String],
    roles: &[RoleSpec],
    targets: &[Vec<String>],
    row: &mut Option<Row>,
    warned: &mut [bool],
    warnings: &mut Vec<String>,
) {
    if stack == query {
        *row = Some(Row {
            values: vec![String::new(); roles.len()],
            gathering: vec![None; roles.len()],
        });
    }
    let Some(current) = row.as_mut() else {
        return;
    };
    for (i, (role, target)) in roles.iter().zip(targets).enumerate() {
        if stack != target.as_slice() {
            continue;
        }
        // A repeated element replaces the earlier value (Qt: last wins).
        if role.attribute.is_empty() {
            current.gathering[i] = Some(String::new());
        } else {
            match attribute(e, &role.attribute) {
                Some(value) => current.values[i] = value,
                None => {
                    current.values[i] = String::new();
                    if !warned[i] {
                        warned[i] = true;
                        warnings.push(format!(
                            "Query error: \"Attribute {} not found\"",
                            role.attribute
                        ));
                    }
                }
            }
        }
    }
}

fn end_element(
    stack: &[String],
    query: &[String],
    targets: &[Vec<String>],
    row: &mut Option<Row>,
    rows: &mut Vec<Vec<String>>,
) {
    let Some(current) = row.as_mut() else {
        return;
    };
    for (i, target) in targets.iter().enumerate() {
        if stack == target.as_slice()
            && let Some(text) = current.gathering[i].take()
        {
            current.values[i] = text;
        }
    }
    if stack == query
        && let Some(done) = row.take()
    {
        rows.push(finish(done));
    }
}

/// Text belongs to every role whose element is open around it: the
/// value of an element is all the text inside it, descendants included.
fn gather(row: &mut Option<Row>, stack: &[String], targets: &[Vec<String>], text: &str) {
    let Some(current) = row.as_mut() else {
        return;
    };
    for (i, target) in targets.iter().enumerate() {
        if stack.len() >= target.len()
            && stack[..target.len()] == *target.as_slice()
            && let Some(buf) = current.gathering[i].as_mut()
        {
            buf.push_str(text);
        }
    }
}

fn finish(mut row: Row) -> Vec<String> {
    for (i, gathering) in row.gathering.iter_mut().enumerate() {
        if let Some(text) = gathering.take() {
            row.values[i] = text;
        }
    }
    row.values
}

fn attribute(e: &BytesStart<'_>, name: &str) -> Option<String> {
    for attr in e.attributes() {
        let attr = attr.ok()?;
        if local(attr.key.as_ref()) == name {
            return attr
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &str, element: &str, attribute: &str) -> RoleSpec {
        RoleSpec {
            name: name.to_owned(),
            element: element_path(element),
            attribute: attribute.to_owned(),
        }
    }

    fn run(xml: &str, query: &str, roles: &[RoleSpec]) -> Parsed {
        parse(xml.as_bytes(), &query_path(query).unwrap(), roles)
    }

    #[test]
    fn query_must_start_with_a_slash() {
        assert_eq!(query_path("rss/item"), None);
        assert_eq!(
            query_path("/rss/item"),
            Some(vec!["rss".to_owned(), "item".to_owned()])
        );
        // `//` is an empty name: it matches nothing, it is not refused.
        assert_eq!(
            run("<rss><item/></rss>", "//item", &[role("t", "title", "")]).rows,
            Vec::<Vec<String>>::new()
        );
    }

    #[test]
    fn rows_read_elements_attributes_and_paths() {
        let xml = "<rss><channel>\
            <item id=\"a1\"><title made=\"2026-01-01\">First</title><info><pages>10</pages></info></item>\
            <item id=\"a2\"><title made=\"2026-01-02\">Second</title><info><pages>20</pages></info></item>\
            </channel></rss>";
        let roles = [
            role("title", "title", ""),
            role("stamp", "title", "made"),
            role("pages", "info/pages", ""),
            role("itemId", "", "id"),
        ];
        let got = run(xml, "/rss/channel/item", &roles);
        assert_eq!(got.warnings, Vec::<String>::new());
        assert_eq!(
            got.rows,
            vec![
                vec!["First", "2026-01-01", "10", "a1"],
                vec!["Second", "2026-01-02", "20", "a2"],
            ]
        );
    }

    #[test]
    fn text_is_all_descendant_text_unescaped_and_last_wins() {
        let xml = "<docs><doc><title>  spaced  </title>\
            <body><![CDATA[<b>raw</b> & co]]></body>\
            <tag>one</tag><tag>two</tag>\
            <nested>outer <inner>IN</inner> tail</nested>\
            <amp>a &amp; b &#38; c</amp></doc></docs>";
        let roles = [
            role("title", "title", ""),
            role("body", "body", ""),
            role("tag", "tag", ""),
            role("nested", "nested", ""),
            role("amp", "amp", ""),
        ];
        let got = run(xml, "/docs/doc", &roles);
        assert_eq!(
            got.rows,
            vec![vec![
                "  spaced  ",
                "<b>raw</b> & co",
                "two",
                "outer IN tail",
                "a & b & c"
            ]]
        );
    }

    #[test]
    fn names_match_by_local_name() {
        let xml = "<rss><channel><item>\
            <media:content url=\"http://x/y.jpg\"/></item></channel></rss>";
        let got = run(
            xml,
            "/rss/channel/item",
            &[role("content", "content", "url")],
        );
        assert_eq!(got.rows, vec![vec!["http://x/y.jpg"]]);
    }

    #[test]
    fn missing_things_read_empty_and_a_missing_attribute_warns_once() {
        let xml = "<docs><doc><title>One</title></doc><doc><title>Two</title></doc></docs>";
        let roles = [
            role("gone", "nope", ""),
            role("noattr", "title", "zzz"),
            role("empty", "missing/deeper", ""),
        ];
        let got = run(xml, "/docs/doc", &roles);
        assert_eq!(got.rows, vec![vec!["", "", ""], vec!["", "", ""]]);
        assert_eq!(
            got.warnings,
            vec!["Query error: \"Attribute zzz not found\"".to_owned()]
        );
    }

    #[test]
    fn a_broken_document_keeps_the_rows_already_read() {
        let xml = "<docs><doc><title>Kept</title></doc><doc><title>Unclosed";
        let got = run(xml, "/docs/doc", &[role("t", "title", "")]);
        assert_eq!(got.rows, vec![vec!["Kept"], vec!["Unclosed"]]);
        assert_eq!(
            got.warnings,
            vec!["Query error: \"Premature end of document.\"".to_owned()]
        );
    }

    #[test]
    fn only_the_exact_path_makes_rows() {
        let xml = "<a><b><item><t>outer</t><b><item><t>inner</t></item></b></item></b></a>";
        let got = run(xml, "/a/b/item", &[role("t", "t", "")]);
        assert_eq!(got.rows, vec![vec!["outer"]]);
    }
}
