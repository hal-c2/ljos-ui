//! Connections, `Binding` and states: what writes a property from outside
//! its own binding. A `Connections` runs statements when its target's
//! properties change, a `Binding` holds a property to a value while its
//! `when` holds and gives the old one back after, and a state's
//! `PropertyChanges` hold properties while the state is the item's.
//! Steps change what they watch, the `when`s and the state.

use crate::{
    bindings::{
        ast::{Changed, Decl, Expr, Kind, Obj, ObjType, PType, Program, Prop, Stmt},
        generate::{Avail, Gen, assign},
        print,
    },
    rng::Rng,
};

/// Connections, `Binding` objects and states over typed properties.
#[derive(Debug, Default)]
pub struct Connections;

crate::program_area!(Connections, "connections", program);

/// The types a property here has: those a `Binding` and a state write
/// without a conversion of their own to be found.
const TYPES: &[PType] = &[
    PType::Int,
    PType::Real,
    PType::Bool,
    PType::String,
    PType::Var,
    PType::Color,
];

const RESTORE: &[&str] = &[
    "Binding.RestoreBindingOrValue",
    "Binding.RestoreBinding",
    "Binding.RestoreValue",
    "Binding.RestoreNone",
];

/// A random program of watched and held properties.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    props(&mut g, &mut root);
    for i in 1..=g.rng.below(3) {
        let id = format!("o{i}");
        let ty = *g.rng.pick(&[ObjType::Item, ObjType::QtObject]);
        let mut o = Obj::new(id.clone(), ty);
        g.ids.push(id);
        props(&mut g, &mut o);
        root.children.push(o);
    }
    let mut made = Vec::new();
    for _ in 0..g.rng.below(3) {
        if let Some(c) = connections(&mut g) {
            made.push(c);
        }
    }
    for _ in 0..g.rng.below(3) {
        if let Some(b) = binding(&mut g) {
            made.push(b);
        }
    }
    let mut states = Vec::new();
    if g.rng.chance(55) {
        let (names, list) = self::states(&mut g);
        states = names;
        root.props.push(list);
        let initial = g
            .rng
            .chance(25)
            .then(|| Expr::Lit(format!("\"{}\"", g.rng.pick(&states)), Kind::Str));
        root.props.push(Prop {
            name: "state".into(),
            decl: Decl::Builtin { ty: PType::String },
            init: initial,
            in_loop: false,
        });
    }
    if g.rng.chance(25) {
        g.after(&mut root);
    }
    let n = 1 + g.rng.below(5);
    let mut steps = Vec::new();
    for _ in 0..n {
        if let Some(s) = step(&mut g, &root, &made, &states) {
            steps.push(s);
        }
    }
    root.children.extend(made);
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}

fn props(g: &mut Gen, o: &mut Obj) {
    for _ in 0..1 + g.rng.below(3) {
        let ty = g.rng.pick(TYPES).clone();
        let init = g.rng.chance(85).then(|| g.init(&ty));
        let name = g.name("p");
        g.add(
            o,
            name,
            Decl::New {
                ty: ty.clone(),
                readonly: false,
            },
            init,
            ty,
            true,
        );
    }
}

fn setting(name: &str, e: Expr) -> Prop {
    Prop::setting(name, e)
}

fn object(id: &str) -> Expr {
    Expr::Ref {
        obj: id.into(),
        prop: String::new(),
        kind: Kind::Any,
    }
}

/// A `Connections` to an object, with handlers of one or two of its
/// properties that log and write a property made after the one watched.
fn connections(g: &mut Gen) -> Option<Obj> {
    let watched = g.pick_scope(|a| a.writable)?;
    let id = g.name("c");
    let mut c = Obj::new(id, ObjType::Connections);
    c.props.push(setting("target", object(&watched.obj)));
    if g.rng.chance(25) {
        let e = g.expr(Kind::Bool, 1, &[]);
        c.props.push(setting("enabled", e));
    }
    let own: Vec<Avail> = g
        .scope
        .iter()
        .filter(|a| a.obj == watched.obj)
        .cloned()
        .collect();
    for _ in 0..1 + g.rng.below(2) {
        let w = g.rng.pick(&own).clone();
        if c.changed.iter().any(|h| h.prop == w.prop) {
            continue;
        }
        let mut body = vec![Stmt::Log {
            text: format!("{}:{}", c.id, w.prop),
            value: Some(Expr::Ref {
                obj: w.obj.clone(),
                prop: w.prop.clone(),
                kind: w.ty.kind(),
            }),
        }];
        if let Some(t) = g.pick_scope(|a| a.writable && a.order > w.order) {
            let value = g.expr(t.ty.kind(), 2, &[]);
            body.push(assign(&t, value));
        }
        c.changed.push(Changed { prop: w.prop, body });
    }
    Some(c)
}

/// A `Binding` holding a property to a value read only from what was made
/// before it, while `when` holds.
fn binding(g: &mut Gen) -> Option<Obj> {
    let t = g.pick_scope(|a| a.writable)?;
    let id = g.name("b");
    let mut b = Obj::new(id, ObjType::Binding);
    b.props.push(setting("target", object(&t.obj)));
    b.props.push(setting(
        "property",
        Expr::Lit(format!("\"{}\"", t.prop), Kind::Str),
    ));
    let saved = g.scope.clone();
    g.scope.retain(|a| a.order < t.order);
    let value = g.expr(t.ty.kind(), 2, &[]);
    g.scope = saved;
    b.props.push(setting("value", value));
    if g.rng.chance(70) {
        let when = g.expr(Kind::Bool, 2, &[]);
        b.props.push(setting("when", when));
    }
    if g.rng.chance(30) {
        let mode = g.lit_from(RESTORE, Kind::Num);
        b.props.push(setting("restoreMode", mode));
    }
    Some(b)
}

/// The root's states, written as one list: each may have a `when`, extend
/// the one before it and change one or two properties, in either form
/// `PropertyChanges` takes.
fn states(g: &mut Gen) -> (Vec<String>, Prop) {
    let mut names = Vec::new();
    let mut text = Vec::new();
    for i in 0..1 + g.rng.below(3) {
        let name = format!("s{i}");
        let mut parts = vec![format!("name: \"{name}\"")];
        if g.rng.chance(30) {
            let when = g.expr(Kind::Bool, 1, &[]);
            parts.push(format!("when: {}", print::expr(&when)));
        }
        if i > 0 && g.rng.chance(25) {
            parts.push(format!("extend: \"s{}\"", i - 1));
        }
        let mut changes = Vec::new();
        let mut changed: Vec<(String, String)> = Vec::new();
        for _ in 0..1 + g.rng.below(2) {
            let Some(t) = g.pick_scope(|a| a.writable) else {
                break;
            };
            // One change of a property in a state; Qt takes the last of two.
            if changed.contains(&(t.obj.clone(), t.prop.clone())) {
                continue;
            }
            changed.push((t.obj.clone(), t.prop.clone()));
            // As a `Binding`'s, read only from what was made before.
            let saved = g.scope.clone();
            g.scope.retain(|a| a.order < t.order);
            let value = g.expr(t.ty.kind(), 1, &[]);
            let value = print::expr(&g.fit(&t.ty, value));
            g.scope = saved;
            let mut extra = String::new();
            if g.rng.chance(15) {
                extra.push_str("; restoreEntryValues: false");
            }
            if g.rng.chance(15) {
                extra.push_str("; explicit: true");
            }
            let change = if g.rng.chance(50) {
                format!(
                    "PropertyChanges {{ target: {}; {}: {value}{extra} }}",
                    t.obj, t.prop
                )
            } else {
                format!("PropertyChanges {{ {}.{}: {value}{extra} }}", t.obj, t.prop)
            };
            changes.push(change);
        }
        // A `;` ends a property, but none may follow an object.
        text.push(format!(
            "State {{ {}; {} }}",
            parts.join("; "),
            changes.join(" ")
        ));
        names.push(name);
    }
    (
        names,
        setting(
            "states",
            Expr::Lit(format!("[{}]", text.join(", ")), Kind::Any),
        ),
    )
}

/// A step: the state, a watched or held property, a `when` or
/// `enabled` read from, or one of the steps of bindings.
fn step(g: &mut Gen, root: &Obj, made: &[Obj], states: &[String]) -> Option<Stmt> {
    match g.rng.below(10) {
        0..3 if !states.is_empty() => {
            let to = if g.rng.chance(20) {
                String::new()
            } else {
                g.rng.pick(states).clone()
            };
            Some(Stmt::Assign {
                obj: "root".into(),
                prop: "state".into(),
                member: None,
                value: Expr::Lit(format!("\"{to}\""), Kind::Str),
            })
        }
        3..7 => {
            let t = g.pick_scope(|a| a.writable)?;
            let value = g.expr(t.ty.kind(), 2, &[]);
            Some(assign(&t, value))
        }
        7 if !made.is_empty() => {
            let o = g.rng.pick(made);
            let prop = if o.ty == ObjType::Binding {
                "when"
            } else {
                "enabled"
            };
            o.props
                .iter()
                .any(|p| p.name == prop)
                .then(|| Stmt::Assign {
                    obj: o.id.clone(),
                    prop: prop.into(),
                    member: None,
                    value: Expr::Lit(g.rng.pick(&["true", "false"]).to_string(), Kind::Bool),
                })
        }
        _ => g.step(root),
    }
}
