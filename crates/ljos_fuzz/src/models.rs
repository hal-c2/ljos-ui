//! Models: a `ListModel` of `ListElement`s, a JavaScript array and a
//! count, each shown by a `Repeater` whose delegate reads its row. Steps
//! change the model through its methods, or the array or count it is
//! given. What is compared is the model's and the repeater's counts, the
//! rows as the model gives them, what each delegate read and the log of
//! the repeater's `itemAdded` and `itemRemoved`.

use crate::{
    bindings::{
        ast::{Decl, Expr, Func, Kind, Obj, ObjType, PType, Program, Prop, Stmt},
        generate::Gen,
    },
    rng::Rng,
};

/// List models and repeaters over them.
#[derive(Debug, Default)]
pub struct Models;

crate::program_area!(Models, "models", program);

const NUMS: &[&str] = &["0", "1", "2.5", "-3", "100", "1e21"];
const STRS: &[&str] = &["\"\"", "\"a\"", "\"Ljós\"", "\"0\"", "\"a b\""];

/// What a `Repeater` is given.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    List,
    Array,
    Count,
}

/// A random program of models and what shows them.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    let mut steps = Vec::new();
    for _ in 0..1 + g.rng.below(2) {
        let source = *g
            .rng
            .pick(&[Source::List, Source::List, Source::Array, Source::Count]);
        let model = match source {
            Source::List => list_model(&mut g, &mut root),
            Source::Array => {
                let name = g.name("p");
                let e = g.lit_from(
                    &[
                        "[]",
                        "[1, 2, 3]",
                        "[\"a\", \"b\"]",
                        "[0.5, -1]",
                        "[[1], {}]",
                    ],
                    Kind::Arr,
                );
                g.add(
                    &mut root,
                    name.clone(),
                    Decl::New {
                        ty: PType::Var,
                        readonly: false,
                    },
                    Some(e),
                    PType::Var,
                    true,
                );
                format!("root.{name}")
            }
            Source::Count => {
                let name = g.name("p");
                let e = g.lit_from(&["0", "1", "3", "-1"], Kind::Num);
                g.add(
                    &mut root,
                    name.clone(),
                    Decl::New {
                        ty: PType::Int,
                        readonly: false,
                    },
                    Some(e),
                    PType::Int,
                    true,
                );
                format!("root.{name}")
            }
        };
        if g.rng.chance(85) {
            repeater(&mut g, &mut root, &model, source);
        }
    }
    for _ in 0..1 + g.rng.below(5) {
        if let Some(s) = step(&mut g, &root) {
            steps.push(s);
        }
    }
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}

/// A `ListModel` with rows of the same roles, its count observed, and
/// functions on the root that change it and read it back.
fn list_model(g: &mut Gen, root: &mut Obj) -> String {
    let id = g.name("m");
    let mut m = Obj::new(id.clone(), ObjType::ListModel);
    m.props.push(Prop {
        name: "count".into(),
        decl: Decl::Builtin { ty: PType::Int },
        init: None,
        in_loop: false,
    });
    for _ in 0..g.rng.below(4) {
        let row_id = g.name("r");
        let mut row = Obj::new(row_id, ObjType::ListElement);
        for (role, from) in [("k", NUMS), ("s", STRS)] {
            row.props.push(Prop {
                name: role.into(),
                decl: Decl::Setting,
                init: Some(g.lit_from(from, Kind::Any)),
                in_loop: false,
            });
        }
        m.children.push(row);
    }
    root.children.push(m);
    let f = |name: String, params: &[(&str, Kind)], tpl: String| Func {
        name,
        params: params
            .iter()
            .map(|(n, k)| ((*n).to_owned(), *k, None))
            .collect(),
        throws: None,
        ret: Expr::op(
            "model-call",
            tpl,
            Kind::Any,
            params
                .iter()
                .map(|(n, k)| Expr::Param((*n).to_owned(), *k))
                .collect(),
        ),
    };
    let (n, s) = (("a", Kind::Num), ("b", Kind::Str));
    root.funcs.extend([
        f(
            format!("{id}_append"),
            &[n, s],
            format!("{id}.append({{k: $0, s: $1}})"),
        ),
        f(
            format!("{id}_insert"),
            &[n, s],
            format!("{id}.insert(0, {{k: $0, s: $1}})"),
        ),
        f(format!("{id}_remove"), &[n], format!("{id}.remove($0)")),
        f(
            format!("{id}_setProperty"),
            &[n, s],
            format!("{id}.setProperty(0, \"s\", $1 + $0)"),
        ),
        f(
            format!("{id}_set"),
            &[n],
            format!("{id}.set(1, {{k: $0}})"),
        ),
        f(format!("{id}_move"), &[], format!("{id}.move(0, 1, 1)")),
        f(format!("{id}_clear"), &[], format!("{id}.clear()")),
        f(
            format!("{id}_rows"),
            &[],
            format!(
                "(function () {{ var a = []; for (var i = 0; i < {id}.count; ++i) a.push([{id}.get(i).k, {id}.get(i).s]); return a; }})()"
            ),
        ),
    ]);
    // The rows are read where the count changes, too.
    let name = g.name("rows");
    root.props.push(Prop {
        name,
        decl: Decl::New {
            ty: PType::Var,
            readonly: false,
        },
        init: Some(Expr::op(
            "model-rows",
            format!("({id}.count, {id}_rows())"),
            Kind::Any,
            vec![],
        )),
        in_loop: false,
    });
    id
}

/// A `Repeater` of `model` whose delegate reads its row, with its count
/// observed, a function reading what every delegate read, and a log of
/// what it added and removed.
fn repeater(g: &mut Gen, root: &mut Obj, model: &str, source: Source) {
    let id = g.name("rep");
    let mut r = Obj::new(id.clone(), ObjType::Repeater);
    r.props.push(Prop {
        name: "model".into(),
        decl: Decl::Setting,
        init: Some(Expr::Lit(model.into(), Kind::Any)),
        in_loop: false,
    });
    r.props.push(Prop {
        name: "count".into(),
        decl: Decl::Builtin { ty: PType::Int },
        init: None,
        in_loop: false,
    });
    if g.rng.chance(50) {
        r.props.push(Prop {
            name: "onItemAdded".into(),
            decl: Decl::Setting,
            init: Some(Expr::Lit(
                format!("function (index, item) {{ root.log += \"{id}+\" + index + \";\"; }}"),
                Kind::Any,
            )),
            in_loop: false,
        });
        r.props.push(Prop {
            name: "onItemRemoved".into(),
            decl: Decl::Setting,
            init: Some(Expr::Lit(
                format!("function (index, item) {{ root.log += \"{id}-\" + index + \";\"; }}"),
                Kind::Any,
            )),
            in_loop: false,
        });
    }
    let did = g.name("d");
    let mut d = Obj::new(did, ObjType::Item);
    let read = match source {
        Source::List => *g.rng.pick(&[
            "k",
            "s",
            "model.k",
            "model.s",
            "index",
            "s + index",
            "k * 2",
        ]),
        Source::Array => {
            *g.rng
                .pick(&["modelData", "index", "model.modelData", "String(modelData)"])
        }
        Source::Count => *g.rng.pick(&["index", "modelData", "model.index"]),
    };
    d.props.push(Prop {
        name: "v".into(),
        decl: Decl::New {
            ty: PType::Var,
            readonly: false,
        },
        init: Some(Expr::Lit(read.into(), Kind::Any)),
        in_loop: false,
    });
    r.children.push(d);
    root.children.push(r);
    root.funcs.push(Func {
        name: format!("{id}_items"),
        params: Vec::new(),
        throws: None,
        ret: Expr::op(
            "repeater-items",
            format!(
                "(function () {{ var a = []; for (var i = 0; i < {id}.count; ++i) a.push({id}.itemAt(i) ? {id}.itemAt(i).v : \"none\"); return a; }})()"
            ),
            Kind::Any,
            vec![],
        ),
    });
}

/// A call of one of the model's functions, a write of the array or count
/// a repeater shows, or a push to the array.
fn step(g: &mut Gen, root: &Obj) -> Option<Stmt> {
    match g.rng.below(10) {
        0..7 if !root.funcs.is_empty() => {
            let f = g.rng.pick(&root.funcs);
            let (name, kinds): (String, Vec<Kind>) =
                (f.name.clone(), f.params.iter().map(|p| p.1).collect());
            let args = kinds
                .iter()
                .map(|k| {
                    if *k == Kind::Num {
                        g.lit_from(&["0", "1", "2", "-1", "7.5"], Kind::Num)
                    } else {
                        g.lit_from(STRS, Kind::Str)
                    }
                })
                .collect();
            Some(Stmt::Call {
                obj: "root".into(),
                fun: name,
                args,
                ret: Kind::Any,
            })
        }
        _ => {
            let t = g.pick_scope(|a| a.writable)?;
            if t.ty == PType::Var && g.rng.chance(50) {
                return Some(Stmt::Push {
                    obj: t.obj,
                    prop: t.prop,
                    value: g.lit_from(&["4", "\"c\""], Kind::Any),
                });
            }
            let value = if t.ty == PType::Int {
                g.lit_from(&["0", "2", "5", "-2"], Kind::Num)
            } else {
                g.lit_from(
                    &["[]", "[9]", "[\"x\", \"y\", \"z\"]", "null", "3"],
                    Kind::Any,
                )
            };
            Some(crate::bindings::generate::assign(&t, value))
        }
    }
}
