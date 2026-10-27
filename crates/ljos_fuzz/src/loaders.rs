//! Loaders and the lifetime of what they make: a `Loader` switched
//! between components, to none and back, turned off and on, and the
//! order in which the objects it makes are completed and destroyed and
//! it tells of them (`onLoaded`, `onItemChanged`, `onStatusChanged`).
//! Every component's object has an int `q`, so what a loader shows can be
//! read and written whichever it is. Loading is synchronous here, so all
//! of it runs within the step that asks for it.

use crate::{
    bindings::{
        ast::{Changed, Decl, Expr, Func, Kind, Obj, ObjType, PType, Program, Prop, Stmt},
        generate::Gen,
    },
    rng::Rng,
};

/// Loaders over a few components, and what their objects log.
#[derive(Debug, Default)]
pub struct Loaders;

crate::program_area!(Loaders, "loaders", program);

const TYPES: &[PType] = &[PType::Int, PType::Real, PType::Bool, PType::String];

/// A random program of components and loaders.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    for _ in 0..1 + g.rng.below(3) {
        let ty = g.rng.pick(TYPES).clone();
        let init = Some(g.init(&ty));
        let name = g.name("p");
        g.add(
            &mut root,
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
    let components: Vec<String> = (0..1 + g.rng.below(3))
        .map(|_| component(&mut g, &mut root))
        .collect();
    let bools: Vec<String> = root
        .props
        .iter()
        .filter(|p| {
            matches!(
                p.decl,
                Decl::New {
                    ty: PType::Bool,
                    ..
                }
            )
        })
        .map(|p| p.name.clone())
        .collect();
    let mut loaders = Vec::new();
    for _ in 0..1 + g.rng.below(2) {
        loaders.push(loader(&mut g, &mut root, &components, &bools));
    }
    let mut steps = Vec::new();
    for _ in 0..1 + g.rng.below(5) {
        steps.extend(step(&mut g, &root, &components, &loaders));
    }
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}

fn setting(name: &str, text: String) -> Prop {
    Prop::setting(name, Expr::Lit(text, Kind::Any))
}

fn prop(name: &str, ty: PType, init: Expr) -> Prop {
    Prop {
        name: name.into(),
        decl: Decl::New {
            ty,
            readonly: false,
        },
        init: Some(init),
        in_loop: false,
    }
}

/// An object that logs when it is completed and destroyed, as `+id` and
/// `-id`.
fn lifetime(o: &mut Obj) {
    o.completed = Some(vec![Stmt::Log {
        text: format!("+{}", o.id),
        value: None,
    }]);
    o.props.push(setting(
        "Component.onDestruction",
        format!("root.log += \"-{};\"", o.id),
    ));
}

/// A component of an object with `q`, bound to a literal or to one of the
/// root's properties, and maybe a child, both of which log their lifetime.
fn component(g: &mut Gen, root: &mut Obj) -> String {
    let cid = g.name("c");
    let mut c = Obj::new(cid.clone(), ObjType::Component);
    let mid = g.name("m");
    let ty = *g.rng.pick(&[
        ObjType::Item,
        ObjType::Item,
        ObjType::QtObject,
        ObjType::Rectangle,
    ]);
    let mut m = Obj::new(mid.clone(), ty);
    let q = if g.rng.chance(50) {
        g.expr(Kind::Num, 1, &[])
    } else {
        g.init(&PType::Int)
    };
    let q = g.fit(&PType::Int, q);
    m.props.push(prop("q", PType::Int, q));
    if g.rng.chance(40) {
        m.changed.push(Changed {
            prop: "q".into(),
            body: vec![Stmt::Log {
                text: format!("{mid}.q="),
                value: Some(Expr::Ref {
                    obj: mid.clone(),
                    prop: "q".into(),
                    kind: Kind::Num,
                }),
            }],
        });
    }
    lifetime(&mut m);
    if ty != ObjType::QtObject && g.rng.chance(35) {
        let mut child = Obj::new(format!("{mid}k"), ObjType::Item);
        lifetime(&mut child);
        m.children.push(child);
    }
    c.children.push(m);
    root.children.push(c);
    cid
}

/// A loader of one of the components, or of none, maybe inactive or
/// active as a bool of the root's is, with handlers that log what it
/// tells, and properties of the root that read what it shows.
fn loader(g: &mut Gen, root: &mut Obj, components: &[String], bools: &[String]) -> String {
    let id = g.name("l");
    let mut l = Obj::new(id.clone(), ObjType::Loader);
    if g.rng.chance(80) {
        l.props
            .push(setting("sourceComponent", g.rng.pick(components).clone()));
    }
    match g.rng.below(10) {
        0 | 1 => l.props.push(setting("active", "false".into())),
        2 | 3 if !bools.is_empty() => {
            l.props
                .push(setting("active", format!("root.{}", g.rng.pick(bools))));
        }
        _ => {}
    }
    let shown = format!("({id}.item ? {id}.item.objectName : \"null\")");
    let handlers = [
        ("onLoaded", format!("root.log += \"loaded:{id};\"")),
        (
            "onItemChanged",
            format!("root.log += \"item:{id}=\" + {shown} + \";\""),
        ),
        (
            "onStatusChanged",
            format!("root.log += \"status:{id}=\" + {id}.status + \";\""),
        ),
        (
            "onActiveChanged",
            format!("root.log += \"active:{id}=\" + {id}.active + \";\""),
        ),
    ];
    for (name, body) in handlers {
        if g.rng.chance(50) {
            l.props.push(setting(name, body));
        }
    }
    root.children.push(l);
    let read = |tpl: String| Expr::op("loader-read", tpl, Kind::Any, vec![]);
    root.props.push(prop(
        &format!("seen_{id}"),
        PType::Var,
        read(format!("({id}.item ? {id}.item.q : \"none\")")),
    ));
    root.props
        .push(prop(&format!("name_{id}"), PType::String, read(shown)));
    root.props.push(prop(
        &format!("status_{id}"),
        PType::Int,
        read(format!("{id}.status")),
    ));
    root.funcs.push(Func {
        name: format!("set_{id}"),
        params: vec![("a".into(), Kind::Num, None)],
        throws: None,
        ret: Expr::op(
            "loader-write",
            format!("({id}.item ? ({id}.item.q = $0) : \"none\")"),
            Kind::Any,
            vec![Expr::Param("a".into(), Kind::Num)],
        ),
    });
    id
}

/// A step: a loader given another component, none or an empty source,
/// turned off or on, its object's `q` written, or one of the steps of
/// bindings, which also writes the bools loaders may be active by.
fn step(g: &mut Gen, root: &Obj, components: &[String], loaders: &[String]) -> Option<Stmt> {
    let l = g.rng.pick(loaders).clone();
    let set = |prop: &str, value: String, k: Kind| Stmt::Assign {
        obj: l.clone(),
        prop: prop.into(),
        member: None,
        value: Expr::Lit(value, k),
    };
    match g.rng.below(12) {
        0..4 => Some(set(
            "sourceComponent",
            g.rng.pick(components).clone(),
            Kind::Any,
        )),
        4 => Some(set(
            "sourceComponent",
            (*g.rng.pick(&["null", "undefined"])).into(),
            Kind::Any,
        )),
        5 => Some(set("source", "\"\"".into(), Kind::Str)),
        6..8 => Some(set(
            "active",
            (*g.rng.pick(&["true", "false"])).into(),
            Kind::Bool,
        )),
        8 | 9 => Some(Stmt::Call {
            obj: "root".into(),
            fun: format!("set_{l}"),
            args: vec![g.expr(Kind::Num, 1, &[])],
            ret: Kind::Any,
        }),
        _ => g.step(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::print;

    #[test]
    fn every_loaded_object_logs_its_lifetime() {
        for seed in 0..200 {
            let p = program(&mut Rng::new(seed));
            let qml = print::render(&p).qml;
            assert!(qml.contains("Loader {"), "{qml}");
            assert!(
                qml.contains("Component.onDestruction: root.log += \"-m"),
                "{qml}"
            );
            assert!(qml.contains("property int q:"), "{qml}");
        }
    }
}
