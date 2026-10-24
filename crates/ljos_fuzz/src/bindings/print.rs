//! Writing a program as QML.
//!
//! The case file has, besides the program, the functions both sides call:
//! `__obs()` returns every observed property as one JSON string and
//! `__step<i>()` runs step `i` and returns its result the same way. Values
//! are serialised in QML, by helpers written once here, so the two engines
//! are compared on what a program sees and not on how each hands a value
//! to its host. A string or url under the case's own directory is written
//! with `$DIR` for it, as the two sides load the case from different places.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use super::ast::{Decl, Expr, Func, Obj, Program, Stmt};
use crate::outcome::Rendered;

/// The case as both sides run it.
#[must_use]
pub fn render(p: &Program) -> Rendered {
    let mut out = String::from("import QtQuick\n\n");
    obj(p, &p.root, 0, true, &mut out);
    let unobserved: BTreeSet<String> = p
        .root
        .all()
        .iter()
        .flat_map(|o| {
            o.props
                .iter()
                .filter(|p| p.in_loop)
                .map(|p| format!("{}.{}", o.id, p.name))
        })
        .collect();
    let mut types: BTreeMap<String, String> = p
        .root
        .all()
        .iter()
        .flat_map(|o| {
            o.props
                .iter()
                .map(|p| (format!("{}.{}", o.id, p.name), p.ty().qml().to_owned()))
        })
        .collect();
    types.insert("root.log".into(), "string".into());
    Rendered {
        qml: out,
        steps: p.steps.len(),
        unobserved,
        types,
    }
}

/// The program alone, as a scenario shows it.
#[must_use]
pub fn clean(p: &Program) -> String {
    let mut out = String::from("import QtQuick\n\n");
    obj(p, &p.root, 0, false, &mut out);
    out
}

/// A step as JavaScript, for reports.
#[must_use]
pub fn step(s: &Stmt) -> String {
    stmt(s)
}

fn pad(depth: usize) -> String {
    "    ".repeat(depth)
}

fn obj(p: &Program, o: &Obj, depth: usize, harness: bool, out: &mut String) {
    let (i, j) = (pad(depth), pad(depth + 1));
    let _ = writeln!(out, "{i}{} {{", o.ty.qml());
    let _ = writeln!(out, "{j}id: {}", o.id);
    let _ = writeln!(out, "{j}objectName: \"{}\"", o.id);
    if depth == 0 {
        let _ = writeln!(out, "{j}property string log: \"\"");
        for (name, keys) in &p.enums {
            let keys: Vec<String> = keys
                .iter()
                .map(|(k, v)| v.map_or_else(|| k.clone(), |v| format!("{k} = {v}")))
                .collect();
            let _ = writeln!(out, "{j}enum {name} {{ {} }}", keys.join(", "));
        }
    }
    for prop in &o.props {
        let init = prop.init.as_ref().map(expr);
        let line = match &prop.decl {
            Decl::New { ty, readonly } => {
                let ro = if *readonly { "readonly " } else { "" };
                let head = format!("{ro}property {} {}", ty.qml(), prop.name);
                init.map_or(head.clone(), |e| format!("{head}: {e}"))
            }
            Decl::Alias { target, .. } => format!("property alias {}: {target}", prop.name),
            Decl::Builtin { .. } | Decl::Enum => match init {
                Some(e) => format!("{}: {e}", prop.name),
                None => continue,
            },
        };
        let _ = writeln!(out, "{j}{line}");
    }
    for f in &o.funcs {
        let _ = writeln!(out, "{j}{}", func(f));
    }
    for s in &o.signals {
        let params: Vec<String> = s
            .params
            .iter()
            .map(|(n, t)| format!("{} {n}", t.qml()))
            .collect();
        let _ = writeln!(out, "{j}signal {}({})", s.name, params.join(", "));
        if let Some(body) = &s.handler {
            let names: Vec<&str> = s.params.iter().map(|(n, _)| n.as_str()).collect();
            let _ = writeln!(
                out,
                "{j}{}: function ({}) {{ {} }}",
                handler_name(&s.name, ""),
                names.join(", "),
                block(body)
            );
        }
    }
    for c in &o.changed {
        let _ = writeln!(
            out,
            "{j}{}: {{ {} }}",
            handler_name(&c.prop, "Changed"),
            block(&c.body)
        );
    }
    if let Some(body) = &o.completed {
        let _ = writeln!(out, "{j}Component.onCompleted: {{ {} }}", block(body));
    }
    for c in &o.children {
        obj(p, c, depth + 1, harness, out);
    }
    if harness && depth == 0 {
        helpers(p, &j, out);
    }
    let _ = writeln!(out, "{i}}}");
}

fn handler_name(name: &str, suffix: &str) -> String {
    let mut c = name.chars();
    let first = c.next().map(|f| f.to_ascii_uppercase()).unwrap_or_default();
    format!("on{first}{}{suffix}", c.as_str())
}

fn func(f: &Func) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(n, _, ann)| ann.map_or_else(|| n.clone(), |a| format!("{n}: {a}")))
        .collect();
    let guard = f.throws.as_ref().map_or(String::new(), |(g, err)| {
        format!("if ({}) throw new {err}(\"{}\"); ", expr(g), f.name)
    });
    format!(
        "function {}({}) {{ {guard}return {}; }}",
        f.name,
        params.join(", "),
        expr(&f.ret)
    )
}

fn block(body: &[Stmt]) -> String {
    body.iter().map(stmt).collect::<Vec<_>>().join(" ")
}

fn stmt(s: &Stmt) -> String {
    let args = |a: &[Expr]| a.iter().map(expr).collect::<Vec<_>>().join(", ");
    match s {
        Stmt::Assign {
            obj,
            prop,
            member,
            value,
        } => match member {
            Some(m) => format!("{obj}.{prop}.{m} = {};", expr(value)),
            None => format!("{obj}.{prop} = {};", expr(value)),
        },
        Stmt::Bind { obj, prop, value } => {
            format!(
                "{obj}.{prop} = Qt.binding(function () {{ return {}; }});",
                expr(value)
            )
        }
        Stmt::Push { obj, prop, value } => format!("{obj}.{prop}.push({});", expr(value)),
        Stmt::Call {
            obj, fun, args: a, ..
        } => format!("{obj}.{fun}({});", args(a)),
        Stmt::Emit { obj, sig, args: a } => format!("{obj}.{sig}({});", args(a)),
        Stmt::Log { text, value } => match value {
            Some(v) => format!("root.log += \"{text}:\" + ({}) + \";\";", expr(v)),
            None => format!("root.log += \"{text};\";"),
        },
    }
}

/// The JavaScript of an expression.
#[must_use]
pub fn expr(e: &Expr) -> String {
    match e {
        Expr::Lit(t, _) => t.clone(),
        Expr::Ref { obj, prop, .. } if prop.is_empty() => obj.clone(),
        Expr::Ref { obj, prop, .. } => format!("{obj}.{prop}"),
        Expr::Param(n, _) => n.clone(),
        Expr::Op { tpl, args, .. } => {
            let mut out = String::with_capacity(tpl.len() + 16);
            let mut chars = tpl.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '$'
                    && let Some(d) = chars.peek().and_then(|d| d.to_digit(10))
                {
                    chars.next();
                    let a = args.get(d as usize).map(expr).unwrap_or_default();
                    // `1.toFixed(2)` does not parse, `typeof x.length` is not
                    // `(typeof x).length` and `--1` is a decrement: what a
                    // template puts before a member or after a sign gets
                    // parentheses.
                    let signed = a.starts_with(['-', '+']);
                    let member = chars.peek() == Some(&'.')
                        && (signed || a.contains(' ') || a.parse::<f64>().is_ok());
                    let sign = out.ends_with(['-', '+']) && signed;
                    if member || sign {
                        let _ = write!(out, "({a})");
                    } else {
                        out.push_str(&a);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

/// The serialising helpers, `__obs` and the steps.
fn helpers(p: &Program, j: &str, out: &mut String) {
    out.push_str(
        &HELPERS
            .replace("\n    ", &format!("\n{j}"))
            .replacen("    ", j, 1),
    );
    let mut obs = String::new();
    let mut keys: Vec<(String, String)> = vec![("root.log".into(), "__ser(root.log)".into())];
    for o in p.root.all() {
        for prop in &o.props {
            let path = format!("{}.{}", o.id, prop.name);
            keys.push((path.clone(), format!("{}({path})", prop.ty().serialiser())));
        }
    }
    for (k, read) in keys {
        let _ = write!(obs, " __k(o, \"{k}\", function () {{ return {read}; }});");
    }
    let _ = writeln!(
        out,
        "{j}function __obs() {{ var o = {{}};{obs} return JSON.stringify(o); }}"
    );
    for (n, s) in p.steps.iter().enumerate() {
        let body = match s {
            Stmt::Call { obj, fun, args, .. } => {
                let args: Vec<String> = args.iter().map(expr).collect();
                format!(
                    "return JSON.stringify(__ser({obj}.{fun}({})));",
                    args.join(", ")
                )
            }
            other => format!("{} return JSON.stringify({{\"$u\": 1}});", stmt(other)),
        };
        let _ = writeln!(
            out,
            "{j}function __step{n}() {{ try {{ {body} }} catch (e) {{ return JSON.stringify({{\"$err\": String(e && e.name)}}); }} }}"
        );
    }
}

const HELPERS: &str = r#"    function __num(v) { return isFinite(v) ? v : {"$n": String(v)}; }
    function __ser(v) {
        if (v === undefined) return {"$u": 1};
        if (v === null) return null;
        var t = typeof v;
        if (t === "number") return __num(v);
        if (t === "string") return __dir(v);
        if (t === "boolean") return v;
        if (t === "function") return {"$fn": 1};
        if (Array.isArray(v) || typeof v.length === "number") return __list(v);
        if (v.objectName !== undefined) return {"$obj": v.objectName};
        if (v.hslHue !== undefined) return __color(v);
        if (v.pixelSize !== undefined) return __font(v);
        if (v.width !== undefined && v.x !== undefined) return __rect(v);
        if (v.width !== undefined) return __size(v);
        if (v.x !== undefined && v.y !== undefined) return __point(v);
        var o = {}, k = Object.keys(v).sort();
        for (var i = 0; i < k.length; ++i) o[k[i]] = __ser(v[k[i]]);
        return o;
    }
    function __list(v) { var a = []; for (var i = 0; i < v.length; ++i) a.push(__ser(v[i])); return a; }
    function __color(v) { return {"$color": String(v)}; }
    function __point(v) { return {x: __num(v.x), y: __num(v.y)}; }
    function __size(v) { return {width: __num(v.width), height: __num(v.height)}; }
    function __rect(v) { return {x: __num(v.x), y: __num(v.y), width: __num(v.width), height: __num(v.height)}; }
    function __font(v) { return {pixelSize: v.pixelSize, bold: v.bold, italic: v.italic, underline: v.underline, strikeout: v.strikeout, weight: v.weight}; }
    function __url(v) { return __dir(String(v)); }
    function __dir(s) { var b = String(Qt.resolvedUrl(".")); return s.indexOf(b) === 0 ? "$DIR/" + s.slice(b.length) : s; }
    function __k(o, k, f) { try { o[k] = f(); } catch (e) { o[k] = {"$err": String(e && e.name)}; } }
"#;
