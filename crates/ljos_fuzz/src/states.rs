//! States and transitions: the order in which a state change runs what
//! it holds. A state's `PropertyChanges`, a `when` that picks a state, a
//! state that extends another and a `StateChangeScript`; transitions
//! between them whose actions take no time (`ScriptAction`,
//! `PropertyAction`, in sequential and parallel groups), so the whole
//! change runs within the step that makes it. What is compared is the
//! log every handler and script writes, which orders what ran, and the
//! properties the states move.

use crate::{
    bindings::{
        ast::{Changed, Decl, Expr, Kind, Obj, ObjType, PType, Program, Prop, Stmt},
        generate::{Gen, assign},
        print,
    },
    rng::Rng,
};

/// States, the transitions between them, and what they log.
#[derive(Debug, Default)]
pub struct States;

crate::program_area!(States, "states", program);

/// The types a property here has: those a `PropertyChanges` and a
/// `PropertyAction` write without a conversion of their own.
const TYPES: &[PType] = &[
    PType::Int,
    PType::Real,
    PType::Bool,
    PType::String,
    PType::Color,
];

/// A random program of states and transitions.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    props(&mut g, &mut root);
    if g.rng.chance(40) {
        let mut o = Obj::new(
            "o1".into(),
            *g.rng.pick(&[ObjType::Item, ObjType::QtObject]),
        );
        g.ids.push("o1".into());
        props(&mut g, &mut o);
        root.children.push(o);
    }
    let watched: Vec<_> = g.scope.clone();
    let names = states(&mut g, &mut root);
    let initial = g
        .rng
        .chance(20)
        .then(|| Expr::Lit(format!("\"{}\"", g.rng.pick(&names)), Kind::Str));
    root.props.push(Prop {
        name: "state".into(),
        decl: Decl::Builtin { ty: PType::String },
        init: initial,
        in_loop: false,
    });
    root.changed.push(Changed {
        prop: "state".into(),
        body: vec![Stmt::Log {
            text: "st=".into(),
            value: Some(Expr::Ref {
                obj: "root".into(),
                prop: "state".into(),
                kind: Kind::Str,
            }),
        }],
    });
    // Handlers of what the states move show where in the change each is
    // written.
    for a in watched {
        if !g.rng.chance(60) {
            continue;
        }
        let handler = Changed {
            prop: a.prop.clone(),
            body: vec![Stmt::Log {
                text: format!("{}={}:", a.obj, a.prop),
                value: Some(Expr::Ref {
                    obj: a.obj.clone(),
                    prop: a.prop.clone(),
                    kind: a.ty.kind(),
                }),
            }],
        };
        if a.obj == "root" {
            root.changed.push(handler);
        } else if let Some(o) = root.children.iter_mut().find(|o| o.id == a.obj) {
            o.changed.push(handler);
        }
    }
    let transitions = if g.rng.chance(75) {
        self::transitions(&mut g, &mut root, &names)
    } else {
        Vec::new()
    };
    let mut steps = Vec::new();
    for _ in 0..1 + g.rng.below(5) {
        steps.extend(step(&mut g, &root, &names, &transitions));
    }
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

fn setting(name: &str, text: String) -> Prop {
    Prop {
        name: name.into(),
        decl: Decl::Setting,
        init: Some(Expr::Lit(text, Kind::Any)),
        in_loop: false,
    }
}

fn log(text: &str) -> String {
    format!("root.log += \"{text};\"")
}

/// The root's states, as one list: each may have a `when`, extend the one
/// before it, change one or two properties and run a named script.
fn states(g: &mut Gen, root: &mut Obj) -> Vec<String> {
    let mut names = Vec::new();
    let mut text = Vec::new();
    for i in 0..1 + g.rng.below(3) {
        let name = format!("s{i}");
        let mut parts = vec![format!("name: \"{name}\"")];
        if g.rng.chance(25) {
            let when = g.expr(Kind::Bool, 1, &[]);
            parts.push(format!("when: {}", print::expr(&when)));
        }
        if i > 0 && g.rng.chance(25) {
            parts.push(format!("extend: \"s{}\"", i - 1));
        }
        let mut objects = Vec::new();
        let mut changed: Vec<(String, String)> = Vec::new();
        for _ in 0..g.rng.below(3) {
            let Some(t) = g.pick_scope(|a| a.writable) else {
                break;
            };
            if changed.contains(&(t.obj.clone(), t.prop.clone())) {
                continue;
            }
            changed.push((t.obj.clone(), t.prop.clone()));
            let value = if g.rng.chance(70) {
                g.init(&t.ty)
            } else {
                let saved = g.scope.clone();
                g.scope.retain(|a| a.order < t.order);
                let e = g.expr(t.ty.kind(), 1, &[]);
                g.scope = saved;
                g.fit(&t.ty, e)
            };
            let mut extra = String::new();
            if g.rng.chance(15) {
                extra.push_str("; restoreEntryValues: false");
            }
            objects.push(format!(
                "PropertyChanges {{ target: {}; {}: {}{extra} }}",
                t.obj,
                t.prop,
                print::expr(&value)
            ));
        }
        if g.rng.chance(40) {
            objects.push(format!(
                "StateChangeScript {{ name: \"script{i}\"; script: {} }}",
                log(&format!("scs{i}"))
            ));
        }
        // A `;` ends a property, but none may follow an object.
        text.push(format!(
            "State {{ {}; {} }}",
            parts.join("; "),
            objects.join(" ")
        ));
        names.push(name);
    }
    root.props
        .push(setting("states", format!("[{}]", text.join(", "))));
    names
}

/// Where a transition goes from or to: any state, one, two or the
/// default one.
fn ends(g: &mut Gen, names: &[String]) -> String {
    match g.rng.below(6) {
        0 | 1 => "\"*\"".into(),
        2 => "\"\"".into(),
        3 if names.len() > 1 => format!("\"{},{}\"", names[0], names[1]),
        _ => format!("\"{}\"", g.rng.pick(names)),
    }
}

/// An action that takes no time, or a group of them.
fn action(g: &mut Gen, t: usize, k: &mut usize, depth: u32) -> String {
    *k += 1;
    let here = format!("t{t}.{k}");
    match g.rng.below(10) {
        0..4 => format!("ScriptAction {{ script: {} }}", log(&here)),
        4 => {
            let i = g.rng.below(3);
            format!("ScriptAction {{ scriptName: \"script{i}\" }}")
        }
        5..8 => match g.pick_scope(|a| a.writable) {
            Some(a) if g.rng.chance(30) => {
                let value = print::expr(&g.init(&a.ty));
                format!(
                    "PropertyAction {{ target: {}; property: \"{}\"; value: {value} }}",
                    a.obj, a.prop
                )
            }
            Some(a) => format!(
                "PropertyAction {{ target: {}; property: \"{}\" }}",
                a.obj, a.prop
            ),
            None => format!("ScriptAction {{ script: {} }}", log(&here)),
        },
        _ if depth < 2 => {
            let group = *g.rng.pick(&["SequentialAnimation", "ParallelAnimation"]);
            let inner: Vec<String> = (0..1 + g.rng.below(3))
                .map(|_| action(g, t, k, depth + 1))
                .collect();
            format!("{group} {{ {} }}", inner.join(" "))
        }
        _ => format!("ScriptAction {{ script: {} }}", log(&here)),
    }
}

/// The root's transitions, each with an id a step can disable it by.
fn transitions(g: &mut Gen, root: &mut Obj, names: &[String]) -> Vec<String> {
    let mut ids = Vec::new();
    let mut text = Vec::new();
    for t in 0..1 + g.rng.below(2) {
        let id = format!("t{t}");
        let mut parts = vec![format!("id: {id}")];
        if g.rng.chance(70) {
            parts.push(format!("from: {}", ends(g, names)));
        }
        if g.rng.chance(70) {
            parts.push(format!("to: {}", ends(g, names)));
        }
        if g.rng.chance(25) {
            parts.push("reversible: true".into());
        }
        if g.rng.chance(10) {
            parts.push("enabled: false".into());
        }
        let mut k = 0;
        let actions: Vec<String> = (0..1 + g.rng.below(3))
            .map(|_| action(g, t, &mut k, 0))
            .collect();
        text.push(format!(
            "Transition {{ {}; {} }}",
            parts.join("; "),
            actions.join(" ")
        ));
        ids.push(id);
    }
    root.props
        .push(setting("transitions", format!("[{}]", text.join(", "))));
    ids
}

/// A step: the state, a property a state moves or a `when` reads, a
/// transition turned off or on, or one of the steps of bindings.
fn step(g: &mut Gen, root: &Obj, names: &[String], transitions: &[String]) -> Option<Stmt> {
    match g.rng.below(10) {
        0..5 => {
            let to = if g.rng.chance(20) {
                String::new()
            } else {
                g.rng.pick(names).clone()
            };
            Some(Stmt::Assign {
                obj: "root".into(),
                prop: "state".into(),
                member: None,
                value: Expr::Lit(format!("\"{to}\""), Kind::Str),
            })
        }
        5..8 => {
            let t = g.pick_scope(|a| a.writable)?;
            let value = g.expr(t.ty.kind(), 1, &[]);
            Some(assign(&t, value))
        }
        8 if !transitions.is_empty() => Some(Stmt::Assign {
            obj: g.rng.pick(transitions).clone(),
            prop: "enabled".into(),
            member: None,
            value: Expr::Lit(g.rng.pick(&["true", "false"]).to_string(), Kind::Bool),
        }),
        _ => g.step(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_program_has_states_and_a_state_step_is_common() {
        let mut with_state_step = 0;
        for seed in 0..200 {
            let p = program(&mut Rng::new(seed));
            let qml = print::render(&p).qml;
            assert!(qml.contains("states: [State {"), "{qml}");
            assert!(qml.contains("onStateChanged"), "{qml}");
            if p.steps.iter().any(
                |s| matches!(s, Stmt::Assign { prop, obj, .. } if prop == "state" && obj == "root"),
            ) {
                with_state_step += 1;
            }
        }
        assert!(with_state_step > 100, "{with_state_step}");
    }
}
