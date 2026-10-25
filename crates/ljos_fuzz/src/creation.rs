//! Objects made while the program runs: `Component.createObject` with and
//! without a parent and initial properties, and `Qt.createQmlObject`, in a
//! binding, in `Component.onCompleted` and in steps. What is compared is
//! what the program reads of what it made: the made object's properties
//! through a `var` that holds it, the root's children, and the log its
//! handlers write.

use crate::{
    bindings::{
        ast::{Changed, Decl, Expr, Func, Kind, Obj, ObjType, PType, Program, Stmt},
        generate::Gen,
    },
    rng::Rng,
};

/// Components made into objects at run time.
#[derive(Debug, Default)]
pub struct Creation;

crate::program_area!(Creation, "creation", program);

const TYPES: &[PType] = &[
    PType::Int,
    PType::Real,
    PType::Bool,
    PType::String,
    PType::Var,
    PType::Color,
    PType::Point,
];

/// What `Qt.createQmlObject` is given: small, as the text cannot shrink.
const SOURCES: &[(&str, &str)] = &[
    ("import QtQuick; Item { property int q: 3 }", "q"),
    (
        "import QtQuick; QtObject { property string q: \\\"made\\\" }",
        "q",
    ),
    (
        "import QtQuick; Item { property real q: parent ? parent.width : -1 }",
        "q",
    ),
];

/// A component, what its object has, and where it is held once made.
struct Made {
    component: String,
    props: Vec<(String, PType)>,
    holder: String,
}

/// A random program that makes objects.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    if g.rng.chance(60) {
        g.add(
            &mut root,
            "width".into(),
            Decl::Builtin { ty: PType::Real },
            Some(Expr::Lit("120".into(), Kind::Num)),
            PType::Real,
            true,
        );
    }
    for _ in 0..1 + g.rng.below(2) {
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
    let mut made = Vec::new();
    for _ in 0..1 + g.rng.below(2) {
        made.push(component(&mut g, &mut root));
    }
    let kids = Expr::op("children-length", "root.children.length", Kind::Num, vec![]);
    g.add(
        &mut root,
        "kids".into(),
        Decl::New {
            ty: PType::Int,
            readonly: false,
        },
        Some(kids),
        PType::Int,
        false,
    );
    if g.rng.chance(30) {
        let (src, prop) = *g.rng.pick(SOURCES);
        let make = Expr::op(
            "createQmlObject",
            format!("Qt.createQmlObject(\"{src}\", root)"),
            Kind::Any,
            vec![],
        );
        let name = g.name("q");
        g.add(
            &mut root,
            name.clone(),
            Decl::New {
                ty: PType::Var,
                readonly: false,
            },
            Some(make),
            PType::Var,
            false,
        );
        reader(&mut g, &mut root, &name, prop);
    }
    let mut completed = Vec::new();
    let mut steps = Vec::new();
    for m in &made {
        match g.rng.below(10) {
            0..3 => completed.push(Stmt::Assign {
                obj: "root".into(),
                prop: m.holder.clone(),
                member: None,
                value: create(&mut g, m),
            }),
            3..9 => steps.push(Stmt::Assign {
                obj: "root".into(),
                prop: m.holder.clone(),
                member: None,
                value: create(&mut g, m),
            }),
            _ => {}
        }
    }
    if !completed.is_empty() {
        root.completed = Some(completed);
    }
    for _ in 0..g.rng.below(4) {
        let m = g.rng.pick(&made);
        let s = match g.rng.below(10) {
            0..4 => Some(Stmt::Assign {
                obj: "root".into(),
                prop: m.holder.clone(),
                member: None,
                value: create(&mut g, m),
            }),
            4..8 if !m.props.is_empty() => {
                let f = format!("set_{}", m.holder);
                root.funcs
                    .iter()
                    .find(|x| x.name == f)
                    .map(|x| x.params[0].1)
                    .map(|k| Stmt::Call {
                        obj: "root".into(),
                        fun: f,
                        args: vec![g.expr(k, 1, &[])],
                        ret: Kind::Any,
                    })
            }
            _ => g.step(&root),
        };
        steps.extend(s);
    }
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}

/// A component holding an object with typed properties bound to the
/// root's and its own, with handlers that log; a `var` of the root that
/// holds what it makes, a property reading one of the made object's, and
/// a function writing it.
fn component(g: &mut Gen, root: &mut Obj) -> Made {
    let cid = g.name("c");
    let mut c = Obj::new(cid.clone(), ObjType::Component);
    let mid = g.name("m");
    let ty = *g
        .rng
        .pick(&[ObjType::Item, ObjType::QtObject, ObjType::Rectangle]);
    let mut m = Obj::new(mid.clone(), ty);
    // What is inside is read by id only inside it.
    let outside = g.scope.clone();
    let mut props = Vec::new();
    for _ in 0..1 + g.rng.below(3) {
        let pty = g.rng.pick(TYPES).clone();
        let init = g.rng.chance(85).then(|| g.init(&pty));
        let name = g.name("p");
        props.push((name.clone(), pty.clone()));
        g.add(
            &mut m,
            name,
            Decl::New {
                ty: pty.clone(),
                readonly: false,
            },
            init,
            pty,
            true,
        );
    }
    if g.rng.chance(50) {
        m.completed = Some(vec![Stmt::Log {
            text: format!("made:{mid}"),
            value: None,
        }]);
    }
    if g.rng.chance(40) {
        let (name, pty) = g.rng.pick(&props).clone();
        m.changed.push(Changed {
            prop: name.clone(),
            body: vec![Stmt::Log {
                text: format!("{mid}:{name}"),
                value: Some(Expr::Ref {
                    obj: mid.clone(),
                    prop: name,
                    kind: pty.kind(),
                }),
            }],
        });
    }
    g.scope = outside;
    c.children.push(m);
    root.children.push(c);

    let holder = g.name("made");
    let init = g.rng.chance(20).then(|| {
        Expr::op(
            "createObject",
            format!("{cid}.createObject(root)"),
            Kind::Any,
            vec![],
        )
    });
    root.props.push(bindings_prop(&holder, PType::Var, init));
    let (read, _) = g.rng.pick(&props).clone();
    reader(g, root, &holder, &read);
    let (write, wty) = g.rng.pick(&props).clone();
    root.funcs.push(Func {
        name: format!("set_{holder}"),
        params: vec![("a".into(), wty.kind(), None)],
        throws: None,
        ret: Expr::op(
            "made-write",
            format!("(root.{holder} ? (root.{holder}.{write} = $0) : \"none\")"),
            Kind::Any,
            vec![Expr::Param("a".into(), wty.kind())],
        ),
    });
    Made {
        component: cid,
        props,
        holder,
    }
}

fn bindings_prop(name: &str, ty: PType, init: Option<Expr>) -> crate::bindings::ast::Prop {
    crate::bindings::ast::Prop {
        name: name.into(),
        decl: Decl::New {
            ty,
            readonly: false,
        },
        init,
        in_loop: false,
    }
}

/// A property of the root that reads `prop` of what `holder` holds.
fn reader(g: &mut Gen, root: &mut Obj, holder: &str, prop: &str) {
    let name = g.name("seen");
    let read = Expr::op(
        "made-read",
        format!("($0 ? $0.{prop} : \"none\")"),
        Kind::Any,
        vec![Expr::Ref {
            obj: "root".into(),
            prop: holder.into(),
            kind: Kind::Any,
        }],
    );
    root.props
        .push(bindings_prop(&name, PType::Var, Some(read)));
}

/// `createObject` of the component: with the root, `null` or nothing as
/// the parent, and maybe initial properties.
fn create(g: &mut Gen, m: &Made) -> Expr {
    let parent = *g.rng.pick(&["root", "root", "null", ""]);
    let mut args = Vec::new();
    let mut inits = Vec::new();
    if !parent.is_empty() && !m.props.is_empty() && g.rng.chance(50) {
        let (name, ty) = g.rng.pick(&m.props).clone();
        // Now and then a value of another kind, which Qt converts or warns
        // about.
        let k = if g.rng.chance(15) {
            *g.rng.pick(&[Kind::Num, Kind::Str, Kind::Bool])
        } else {
            ty.kind()
        };
        args.push(g.expr(k, 1, &[]));
        inits.push(format!("{name}: $0"));
    }
    let tpl = if inits.is_empty() {
        format!("{}.createObject({parent})", m.component)
    } else {
        format!(
            "{}.createObject({parent}, {{{}}})",
            m.component,
            inits.join(", ")
        )
    };
    Expr::op("createObject", tpl, Kind::Any, args)
}
