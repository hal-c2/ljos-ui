//! Whether two parsed texts are the same document. The formatter runs this
//! on its own output before it answers.
//!
//! QML nodes are compared here, field by field. JavaScript nodes are
//! compared with oxc's `ContentEq`, which looks at every node and value and
//! at no span, so a change of spacing is equal and a change of a token is
//! not. Comments are compared as text, a line at a time without the white
//! space around it (a block comment moves with the member it is in).

use oxc_span::ContentEq;
use qml_syntax::{
    Comment, Parsed,
    ast::{
        QmlBindingValue, QmlDocument, QmlImportSource, QmlMember, QmlObject, QmlQualifiedName,
        QmlTypeName,
    },
};

type Same = Result<(), String>;

fn differ(what: impl std::fmt::Display, a: impl std::fmt::Debug, b: impl std::fmt::Debug) -> Same {
    Err(format!("{what}: {a:?} became {b:?}"))
}

fn check<T: PartialEq + std::fmt::Debug>(what: &str, a: T, b: T) -> Same {
    if a == b { Ok(()) } else { differ(what, a, b) }
}

pub(crate) fn documents(sa: &str, a: &Parsed<'_>, sb: &str, b: &Parsed<'_>) -> Same {
    header(&a.document, &b.document)?;
    object(&a.document.root, &b.document.root, "the root object")?;
    comments(sa, &a.comments, sb, &b.comments)
}

/// One line of the header: what it is and what it says.
fn header_lines(doc: &QmlDocument<'_>) -> Vec<(u32, String)> {
    let mut lines: Vec<(u32, String)> = Vec::new();
    for p in &doc.pragmas {
        lines.push((p.span.start, format!("pragma {} {:?}", p.name, p.value)));
    }
    for i in &doc.imports {
        let source = match &i.source {
            QmlImportSource::Module(name) => format!("module {name}"),
            QmlImportSource::Path(path) => format!("path {path:?}"),
        };
        let alias = i.alias.as_ref().map(|a| a.name.as_str());
        lines.push((
            i.span.start,
            format!("import {source} {:?} {alias:?}", i.version),
        ));
    }
    lines.sort();
    lines
}

fn header(a: &QmlDocument<'_>, b: &QmlDocument<'_>) -> Same {
    let (la, lb) = (header_lines(a), header_lines(b));
    let (la, lb): (Vec<&str>, Vec<&str>) = (
        la.iter().map(|l| l.1.as_str()).collect(),
        lb.iter().map(|l| l.1.as_str()).collect(),
    );
    check("the pragmas and imports", la, lb)
}

fn name(a: &QmlQualifiedName<'_>, b: &QmlQualifiedName<'_>, what: &str) -> Same {
    check(what, a.to_string(), b.to_string())
}

fn type_name(a: Option<&QmlTypeName<'_>>, b: Option<&QmlTypeName<'_>>, what: &str) -> Same {
    let say = |t: Option<&QmlTypeName<'_>>| t.map(|t| (t.name.to_string(), t.is_list));
    check(what, say(a), say(b))
}

fn object(a: &QmlObject<'_>, b: &QmlObject<'_>, at: &str) -> Same {
    name(&a.type_name, &b.type_name, "a type name")?;
    let at = format!("{at} > {}", a.type_name);
    let on = |o: &QmlObject<'_>| o.on.as_ref().map(ToString::to_string);
    check("the target of `on`", on(a), on(b))?;
    if a.members.len() != b.members.len() {
        return differ(
            format!("the number of members of {at}"),
            a.members.len(),
            b.members.len(),
        );
    }
    for (ma, mb) in a.members.iter().zip(&b.members) {
        member(ma, mb, &at)?;
    }
    Ok(())
}

fn member(a: &QmlMember<'_>, b: &QmlMember<'_>, at: &str) -> Same {
    match (a, b) {
        (QmlMember::Object(a), QmlMember::Object(b)) => object(a, b, at),
        (QmlMember::Binding(a), QmlMember::Binding(b)) => {
            name(&a.name, &b.name, "a binding's name")?;
            value(&a.value, &b.value, &format!("{at} > {}", a.name))
        }
        (QmlMember::Property(a), QmlMember::Property(b)) => {
            let at = format!("{at} > property {}", a.name.name);
            check(&at, a.name.name.as_str(), b.name.name.as_str())?;
            check(
                &at,
                (a.is_default, a.is_required, a.is_readonly),
                (b.is_default, b.is_required, b.is_readonly),
            )?;
            type_name(a.type_name.as_ref(), b.type_name.as_ref(), &at)?;
            match (&a.value, &b.value) {
                (None, None) => Ok(()),
                (Some(a), Some(b)) => value(a, b, &at),
                _ => Err(format!("{at}: its value came or went")),
            }
        }
        (QmlMember::Signal(a), QmlMember::Signal(b)) => {
            let at = format!("{at} > signal {}", a.name.name);
            check(&at, a.name.name.as_str(), b.name.name.as_str())?;
            check(&at, a.params.len(), b.params.len())?;
            for (pa, pb) in a.params.iter().zip(&b.params) {
                check(&at, pa.name.name.as_str(), pb.name.name.as_str())?;
                type_name(pa.type_name.as_ref(), pb.type_name.as_ref(), &at)?;
            }
            Ok(())
        }
        (QmlMember::Function(a), QmlMember::Function(b)) => {
            if a.content_eq(b) {
                Ok(())
            } else {
                Err(format!("{at}: a function is not what it was"))
            }
        }
        (QmlMember::InlineComponent(a), QmlMember::InlineComponent(b)) => {
            check(
                "a component's name",
                a.name.name.as_str(),
                b.name.name.as_str(),
            )?;
            object(
                &a.object,
                &b.object,
                &format!("{at} > component {}", a.name.name),
            )
        }
        (QmlMember::Enum(a), QmlMember::Enum(b)) => {
            let at = format!("{at} > enum {}", a.name.name);
            check(&at, a.name.name.as_str(), b.name.name.as_str())?;
            let keys = |e: &qml_syntax::ast::QmlEnumDeclaration<'_>| {
                e.members
                    .iter()
                    .map(|m| (m.name.name.to_string(), m.value))
                    .collect::<Vec<_>>()
            };
            check(&at, keys(a), keys(b))
        }
        _ => Err(format!("{at}: a member became another kind of member")),
    }
}

fn value(a: &QmlBindingValue<'_>, b: &QmlBindingValue<'_>, at: &str) -> Same {
    let same = match (a, b) {
        (QmlBindingValue::Expression(a), QmlBindingValue::Expression(b)) => a.content_eq(b),
        (QmlBindingValue::Statement(a), QmlBindingValue::Statement(b)) => a.content_eq(b),
        (QmlBindingValue::Object(a), QmlBindingValue::Object(b)) => return object(a, b, at),
        (QmlBindingValue::Objects(a), QmlBindingValue::Objects(b)) => {
            check(&format!("the objects of {at}"), a.len(), b.len())?;
            for (oa, ob) in a.iter().zip(b) {
                object(oa, ob, at)?;
            }
            return Ok(());
        }
        _ => return Err(format!("{at}: its value became another kind of value")),
    };
    if same {
        Ok(())
    } else {
        Err(format!("{at}: its JavaScript is not what it was"))
    }
}

/// A comment as its lines, each without the white space around it.
fn comment_lines<'s>(source: &'s str, comment: &Comment) -> Vec<&'s str> {
    source[comment.span.start as usize..comment.span.end as usize]
        .lines()
        .map(str::trim)
        .collect()
}

fn comments(sa: &str, a: &[Comment], sb: &str, b: &[Comment]) -> Same {
    if a.len() != b.len() {
        return differ("the number of comments", a.len(), b.len());
    }
    for (ca, cb) in a.iter().zip(b) {
        let (la, lb) = (comment_lines(sa, ca), comment_lines(sb, cb));
        if la != lb {
            return differ("a comment", la, lb);
        }
    }
    Ok(())
}
