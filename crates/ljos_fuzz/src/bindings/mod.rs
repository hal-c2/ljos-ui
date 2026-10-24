//! The first area: bindings and JavaScript.

pub mod ast;
pub mod generate;
pub mod print;
pub mod shrink;

use std::{collections::BTreeSet, fmt::Write};

use serde_json::Value;

use self::ast::{Decl, Expr, Program, Stmt};
use crate::{
    area::Area,
    compare::{Diff, DiffKind, message_shape},
    outcome::Rendered,
    rng::Rng,
};

/// Object trees with typed properties bound to JavaScript, aliases,
/// functions, signals, handlers and a list of steps run after the load.
#[derive(Debug, Default)]
pub struct Bindings;

impl Area for Bindings {
    type Case = Program;

    fn name(&self) -> &'static str {
        "bindings"
    }

    fn generate(&self, rng: &mut Rng) -> Program {
        generate::program(rng)
    }

    fn render(&self, case: &Program) -> Rendered {
        print::render(case)
    }

    fn reductions(&self, case: &Program) -> Vec<Program> {
        shrink::reductions(case)
    }

    fn describe(&self, case: &Program, diff: &Diff) -> (String, String) {
        describe(case, diff)
    }

    fn program(&self, case: &Program) -> String {
        print::clean(case)
    }

    fn steps(&self, case: &Program) -> Vec<String> {
        case.steps.iter().map(print::step).collect()
    }

    fn summary(&self, case: &Program) -> String {
        let objs = case.root.all();
        let props: usize = objs.iter().map(|o| o.props.len()).sum();
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        format!(
            "{}, {}, {}",
            plural(objs.len(), "object"),
            plural(props, "property").replace("propertys", "properties"),
            plural(case.steps.len(), "step")
        )
    }

    fn scenario(&self, case: &Program, diff: &Diff, signature: &str) -> String {
        scenario(case, diff, signature)
    }
}

/// The value type and the construct of a difference, the two parts of a
/// bucket's signature besides the kind of difference.
fn describe(p: &Program, d: &Diff) -> (String, String) {
    let step = d.phase.checked_sub(1).and_then(|i| p.steps.get(i));
    let step_label = |s: &Stmt| {
        let first = s.exprs().first().map_or("none", |e| e.label());
        format!("step-{}({first})", s.label())
    };
    match d.kind {
        DiffKind::Value | DiffKind::Missing => {
            if d.key == "root.log" {
                return ("log".into(), handlers(p));
            }
            let Some((obj, name)) = d.key.split_once('.') else {
                return ("?".into(), "?".into());
            };
            let Some(prop) = p.prop(obj, name) else {
                return ("?".into(), "?".into());
            };
            let mut ty = prop.ty().qml().to_owned();
            if ty == "var" {
                let _ = write!(ty, "({})", json_kind(&d.qt));
            }
            let construct = match step {
                Some(s) => step_label(s),
                None => match (&prop.decl, &prop.init) {
                    (Decl::Alias { target, .. }, _) if target.matches('.').count() > 1 => {
                        "alias-to-member".into()
                    }
                    (Decl::Alias { .. }, _) => "alias".into(),
                    (_, Some(e)) => e.label().to_owned(),
                    (_, None) => "default".into(),
                },
            };
            (ty, construct)
        }
        DiffKind::Result | DiffKind::Throws | DiffKind::ErrorKind => {
            let err = |v: &Value| {
                v.get("$err")
                    .and_then(Value::as_str)
                    .unwrap_or("none")
                    .to_owned()
            };
            let ty = format!("{}/{}", err(&d.qt), err(&d.ljos));
            let construct = match step {
                Some(Stmt::Call { obj, fun, .. }) => p
                    .obj(obj)
                    .and_then(|o| o.funcs.iter().find(|f| f.name == *fun))
                    .map_or_else(
                        || "call".to_owned(),
                        |f| match &f.throws {
                            Some(_) => "call-throwing".into(),
                            None => format!("call({})", f.ret.label()),
                        },
                    ),
                Some(s @ Stmt::Assign { obj, prop, .. }) => {
                    let ro = p.prop(obj, prop).is_some_and(|p| p.readonly());
                    if ro {
                        "step-set-readonly".into()
                    } else {
                        step_label(s)
                    }
                }
                Some(s) => step_label(s),
                None => "load".into(),
            };
            (ty, construct)
        }
        DiffKind::Warning => {
            let (side, text) = if d.qt.is_null() {
                ("ljos-only", &d.ljos)
            } else {
                ("qt-only", &d.qt)
            };
            // A warning says what it is about better than the program does;
            // only a binding loop's says no more than the property's name,
            // which is worth keeping where it is an item's own, and Qt's loop
            // for a binding reading an alias and its target together is one
            // bug whatever else the program does.
            let construct = match text.as_str() {
                Some(t) if d.key != "binding-loop" => message_shape(t),
                Some(t) => looped_builtin(t)
                    .or_else(|| reads_alias_and_target(p).then(|| "alias-and-target".into()))
                    .unwrap_or_else(|| step.map_or_else(|| features(p), step_label)),
                None => step.map_or_else(|| features(p), step_label),
            };
            (format!("{}({side})", d.key), construct)
        }
        DiffKind::LjosRejects | DiffKind::LjosCrash | DiffKind::Phases => ("-".into(), features(p)),
    }
}

/// The property a binding loop names, when it is not one the generator made
/// (`p3`, `a7`): `implicitHeight` reading `height` is one bucket however the
/// program around it reads.
fn looped_builtin(text: &str) -> Option<String> {
    let rest = text.split_once("property \"")?.1;
    let name = rest.split_once('"')?.0;
    let generated = name.len() > 1
        && name.starts_with(['p', 'a'])
        && name[1..].bytes().all(|b| b.is_ascii_digit());
    (!generated && !name.is_empty()).then(|| name.to_owned())
}

/// An object's id and one of its properties.
type Named<'a> = (&'a str, &'a str);

/// Whether some binding reads both an alias and the property it is of.
fn reads_alias_and_target(p: &Program) -> bool {
    let objs = p.root.all();
    let aliases: Vec<(Named, Named)> = objs
        .iter()
        .flat_map(|o| {
            o.props.iter().filter_map(|prop| match &prop.decl {
                Decl::Alias { target, .. } => {
                    let mut parts = target.split('.');
                    let (obj, name) = (parts.next()?, parts.next()?);
                    Some(((o.id.as_str(), prop.name.as_str()), (obj, name)))
                }
                _ => None,
            })
        })
        .collect();
    objs.iter()
        .flat_map(|o| o.props.iter().filter_map(|prop| prop.init.as_ref()))
        .any(|e| {
            let mut refs = BTreeSet::new();
            e.visit(&mut |n| {
                if let Expr::Ref { obj, prop, .. } = n {
                    refs.insert((obj.clone(), prop.clone()));
                }
            });
            let has = |(o, n): (&str, &str)| refs.contains(&(o.to_owned(), n.to_owned()));
            aliases.iter().any(|&(a, t)| has(a) && has(t))
        })
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(o) if o.contains_key("$u") => "undefined",
        Value::Object(o) if o.contains_key("$obj") => "object-ref",
        Value::Object(o) if o.contains_key("$color") => "color",
        Value::Object(o) if o.contains_key("$n") => "number",
        Value::Object(_) => "object",
    }
}

/// What in a (small) program writes the log, for a log difference.
fn handlers(p: &Program) -> String {
    let mut kinds = BTreeSet::new();
    for o in p.root.all() {
        if o.completed.is_some() {
            kinds.insert("onCompleted");
        }
        if !o.changed.is_empty() {
            kinds.insert("onChanged");
        }
        if o.signals.iter().any(|s| s.handler.is_some()) {
            kinds.insert("signal-handler");
        }
    }
    if kinds.is_empty() {
        "log".into()
    } else {
        kinds.into_iter().collect::<Vec<_>>().join("+")
    }
}

/// The constructs a whole (small) program uses, for a difference that is
/// not about one property.
fn features(p: &Program) -> String {
    let mut labels = BTreeSet::new();
    let mut copy = p.clone();
    copy.exprs_mut(&mut |e| e.labels(&mut labels));
    for l in ["literal", "ref", "param"] {
        labels.remove(l);
    }
    if p.root
        .all()
        .iter()
        .any(|o| o.props.iter().any(|p| p.in_loop))
    {
        labels.insert("binding-loop");
    }
    if p.root
        .all()
        .iter()
        .any(|o| o.props.iter().any(|p| matches!(p.decl, Decl::Alias { .. })))
    {
        labels.insert("alias");
    }
    if labels.is_empty() {
        return "literal".into();
    }
    labels.into_iter().take(3).collect::<Vec<_>>().join("+")
}

/// A draft of the `@qt-parity` scenario for a bucket, in catalogue steps
/// where there is one; what has none is left as a comment for the author.
fn scenario(p: &Program, d: &Diff, signature: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "  @qt-parity @backlog");
    let _ = writeln!(s, "  Scenario: TODO describe {signature}");
    let _ = writeln!(
        s,
        "    # Sources: found by ljos-fuzz (bindings); bucket {signature}"
    );
    let _ = writeln!(s, "    # Verified on Qt 6.11.2 (qml6, offscreen).");
    let _ = writeln!(s, "    Given the QML:");
    let _ = writeln!(s, "      \"\"\"qml");
    for line in print::clean(p).lines() {
        if line.is_empty() {
            let _ = writeln!(s);
        } else {
            let _ = writeln!(s, "      {line}");
        }
    }
    let _ = writeln!(s, "      \"\"\"");
    for st in p.steps.iter().take(d.phase) {
        let _ = writeln!(s, "    {}", catalogue_step(st));
    }
    match d.kind {
        DiffKind::Value if d.key.split_once('.').is_some() && plain(&d.qt) => {
            let (obj, prop) = d.key.split_once('.').expect("checked");
            let _ = writeln!(
                s,
                "    Then the property \"{prop}\" of \"{obj}\" is {}",
                d.qt
            );
        }
        DiffKind::Warning if !d.qt.is_null() => {
            let _ = writeln!(
                s,
                "    Then the log has a warning matching \"{}\"",
                warning_pattern(&d.key)
            );
        }
        DiffKind::Warning => {
            let _ = writeln!(
                s,
                "    Then the log has no warning matching \"{}\"",
                warning_pattern(&d.key)
            );
        }
        _ => {
            let _ = writeln!(s, "    # Then: Qt {} where Ljós {}", d.qt, d.ljos);
        }
    }
    s
}

fn warning_pattern(category: &str) -> &str {
    match category {
        "binding-loop" => "(?i)binding loop",
        "assign" => "(?i)assign",
        other => other,
    }
}

/// A value a `Then the property … is V` step can say.
fn plain(v: &Value) -> bool {
    match v {
        Value::Object(_) => false,
        Value::Array(a) => a.iter().all(plain),
        _ => true,
    }
}

fn catalogue_step(st: &Stmt) -> String {
    let lit = |e: &ast::Expr| {
        let text = print::expr(e);
        serde_json::from_str::<Value>(&text).ok().map(|_| text)
    };
    match st {
        Stmt::Assign {
            obj,
            prop,
            member: None,
            value,
        } => match lit(value) {
            Some(v) => format!("When the property \"{prop}\" of \"{obj}\" is set to {v}"),
            None => format!("# When (no catalogue step): {}", print::step(st)),
        },
        Stmt::Bind { obj, prop, value } if !print::expr(value).contains('"') => {
            format!(
                "When the property \"{prop}\" of \"{obj}\" is bound to \"{}\"",
                print::expr(value)
            )
        }
        Stmt::Call {
            obj, fun: m, args, ..
        }
        | Stmt::Emit { obj, sig: m, args } => {
            let vals: Option<Vec<String>> = args.iter().map(lit).collect();
            match vals {
                Some(v) if v.is_empty() => {
                    format!("When the method \"{m}\" of \"{obj}\" is called")
                }
                Some(v) => format!(
                    "When the method \"{m}\" of \"{obj}\" is called with {}",
                    v.join(", ")
                ),
                None => format!("# When (no catalogue step): {}", print::step(st)),
            }
        }
        _ => format!("# When (no catalogue step): {}", print::step(st)),
    }
}

#[cfg(test)]
mod tests {
    use super::looped_builtin;

    #[test]
    fn a_loop_is_keyed_by_an_items_own_property_only() {
        let loop_on = |name: &str| {
            looped_builtin(&format!(
                "file:///C.qml:7:5: QML Item: Binding loop detected for property \"{name}\":"
            ))
        };
        assert_eq!(loop_on("implicitHeight").as_deref(), Some("implicitHeight"));
        assert_eq!(loop_on("font.italic").as_deref(), Some("font.italic"));
        assert_eq!(loop_on("p12"), None);
        assert_eq!(loop_on("a3"), None);
        assert_eq!(loop_on("p"), Some("p".to_owned()));
    }
}
