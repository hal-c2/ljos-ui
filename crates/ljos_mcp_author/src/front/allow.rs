//! `// ljos-allow(<code>): <reason>`: a finding its author has looked at
//! and keeps, with the reason written where the next reader sees it.
//!
//! An allow applies to a finding of its code on its own line, on the line
//! below it, or anywhere in the object whose first line it is on (or is
//! just above). It keeps a warning or a hint; an error is certain, so an
//! allow never hides one. An allow with no reason is itself an error
//! (`lint-allow-without-reason`): the point is the reason. An allow that
//! keeps nothing is a hint (`lint-allow-unused`), so allows do not outlive
//! the code they were for.

use std::path::Path;

use qml_syntax::ast::{QmlBindingValue, QmlMember, QmlObject};

use super::diagnostic;
use crate::{Diagnostic, Severity, lint, lints};

/// One allow comment.
struct Allow<'s> {
    /// 1-based.
    line: u32,
    offset: u32,
    code: &'s str,
    reason: bool,
    used: bool,
}

const MARK: &str = "ljos-allow(";

fn allows(source: &str) -> Vec<Allow<'_>> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (n, text) in source.split('\n').enumerate() {
        let line_start = start;
        start += text.len() + 1;
        let Some(comment) = text.find("//") else {
            continue;
        };
        let Some(at) = text[comment..].find(MARK).map(|i| comment + i) else {
            continue;
        };
        let rest = &text[at + MARK.len()..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        let code = rest[..close].trim();
        let after = rest[close + 1..].trim_start();
        let reason = after
            .strip_prefix(':')
            .is_some_and(|why| !why.trim().is_empty());
        out.push(Allow {
            line: n as u32 + 1,
            offset: (line_start + comment) as u32,
            code,
            reason,
            used: false,
        });
    }
    out
}

/// The first and last line of every object in the file.
fn objects(source: &str, object: &QmlObject<'_>, out: &mut Vec<(u32, u32)>) {
    let line = |offset: u32| qml_syntax::position(source, offset).0 as u32;
    out.push((line(object.span.start), line(object.span.end)));
    for member in &object.members {
        match member {
            QmlMember::Object(child) => objects(source, child, out),
            QmlMember::InlineComponent(inline) => objects(source, &inline.object, out),
            QmlMember::Binding(binding) => values(source, &binding.value, out),
            QmlMember::Property(p) => {
                if let Some(value) = &p.value {
                    values(source, value, out);
                }
            }
            _ => {}
        }
    }
}

fn values(source: &str, value: &QmlBindingValue<'_>, out: &mut Vec<(u32, u32)>) {
    match value {
        QmlBindingValue::Object(o) => objects(source, o, out),
        QmlBindingValue::Objects(os) => os.iter().for_each(|o| objects(source, o, out)),
        _ => {}
    }
}

/// Takes the findings in `out` that an allow keeps, and adds what is wrong
/// with the allows themselves.
pub(super) fn apply(path: &Path, source: &str, root: &QmlObject<'_>, out: &mut Vec<Diagnostic>) {
    let mut allows = allows(source);
    if allows.is_empty() {
        return;
    }
    let mut spans = Vec::new();
    objects(source, root, &mut spans);
    out.retain(|d| {
        if d.severity == Severity::Error || lint(&d.code).is_none() {
            return true;
        }
        // Every object the finding is in: an allow above a `Rectangle`
        // keeps what its `MouseArea` is found to do.
        let opens = |line: u32| {
            spans.iter().any(|(start, end)| {
                (line == *start || line + 1 == *start) && *start <= d.line && d.line <= *end
            })
        };
        let applies = |line: u32| line == d.line || line + 1 == d.line || opens(line);
        let mut kept = false;
        for allow in allows.iter_mut() {
            if allow.code == d.code && applies(allow.line) {
                allow.used = true;
                kept = true;
            }
        }
        !kept
    });
    for allow in &allows {
        if !allow.reason {
            let rule = lints::design("lint-allow-without-reason");
            out.push(diagnostic(
                path,
                source,
                allow.offset,
                rule.severity,
                rule.code,
                format!(
                    "`ljos-allow({})` gives no reason; write why the finding is right here \
                     after a colon: `// ljos-allow({}): <why>`",
                    allow.code, allow.code
                ),
            ));
        }
        if !allow.used {
            let rule = lints::design("lint-allow-unused");
            let what = match lint(allow.code) {
                None => format!("`{}` is not a lint code", allow.code),
                Some(l) if l.severity == Severity::Error => format!(
                    "`{}` is certain, and an allow never keeps it; fix what it finds",
                    allow.code
                ),
                Some(_) => format!(
                    "no `{}` finding is on its line, the line below, or in the object it opens",
                    allow.code
                ),
            };
            out.push(diagnostic(
                path,
                source,
                allow.offset,
                rule.severity,
                rule.code,
                format!("this allow keeps nothing: {what}"),
            ));
        }
    }
    out.sort_by(|a, b| {
        (a.line, a.column, &a.code, &a.message).cmp(&(b.line, b.column, &b.code, &b.message))
    });
}
