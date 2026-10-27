//! Geometry: items placed by `x`, `y`, sizes and bindings to their
//! parent's, by anchors to their parent or a sibling with margins and
//! offsets, and by `Row` and `Column`. What is compared is where every
//! item ends up, once made and after each step moves something.
//!
//! A text is left out: its size is its font's, which the two sides measure
//! with different fonts.

use crate::{
    bindings::{
        self,
        ast::{Decl, Expr, Kind, Obj, ObjType, PType, Program, Stmt},
        generate::Gen,
    },
    compare::{Diff, DiffKind},
    rng::Rng,
};

/// Items placed by size, anchors and positioners.
#[derive(Debug, Default)]
pub struct Geometry;

crate::program_area!(Geometry, "geometry", program, describe);

const HORIZONTAL: &[&str] = &["left", "right", "horizontalCenter"];
const VERTICAL: &[&str] = &["top", "bottom", "verticalCenter"];
const MARGINS: &[&str] = &[
    "margins",
    "leftMargin",
    "rightMargin",
    "topMargin",
    "bottomMargin",
    "horizontalCenterOffset",
    "verticalCenterOffset",
];
const SIZES: &[&str] = &["0", "10", "25", "40", "100", "7.5", "-5", "33.3"];

/// A random tree of placed items.
pub fn program(rng: &mut Rng) -> Program {
    let mut g = Gen::new(rng);
    let mut root = Obj::new("root".into(), ObjType::Item);
    g.ids.push("root".into());
    for name in ["width", "height"] {
        let size = Expr::Lit(
            g.rng.pick(&["200", "100", "150", "64"]).to_string(),
            Kind::Num,
        );
        g.add(
            &mut root,
            name.into(),
            Decl::Builtin { ty: PType::Real },
            Some(size),
            PType::Real,
            true,
        );
    }
    for _ in 0..g.rng.below(3) {
        let name = g.name("p");
        let e = g.lit_from(SIZES, Kind::Num);
        g.add(
            &mut root,
            name,
            Decl::New {
                ty: PType::Real,
                readonly: false,
            },
            Some(e),
            PType::Real,
            true,
        );
    }
    let mut count = 1;
    let target = 2 + g.rng.below(4);
    children(&mut g, &mut root, 0, &mut count, target);
    if g.rng.chance(30) {
        g.after(&mut root);
    }
    let n = g.rng.below(5);
    let steps = (0..n).filter_map(|_| step(&mut g, &root)).collect();
    Program {
        root,
        enums: Vec::new(),
        steps,
    }
}

fn children(g: &mut Gen, parent: &mut Obj, depth: usize, count: &mut usize, target: usize) {
    let positioner = matches!(parent.ty, ObjType::Row | ObjType::Column);
    let mut siblings: Vec<String> = Vec::new();
    while *count < target && depth < 3 && (siblings.is_empty() || g.rng.chance(65)) {
        *count += 1;
        let ty = if depth < 2 && g.rng.chance(25) {
            *g.rng.pick(&[ObjType::Row, ObjType::Column])
        } else {
            *g.rng.pick(&[ObjType::Item, ObjType::Rectangle])
        };
        let id = format!("o{}", *count - 1);
        let mut o = Obj::new(id.clone(), ty);
        g.ids.push(id.clone());
        place(g, &mut o, parent, &siblings, positioner);
        if matches!(ty, ObjType::Row | ObjType::Column) && g.rng.chance(60) {
            let e = g.lit_from(&["0", "4", "2.5", "-3"], Kind::Num);
            g.add(
                &mut o,
                "spacing".into(),
                Decl::Builtin { ty: PType::Real },
                Some(e),
                PType::Real,
                true,
            );
        }
        if matches!(ty, ObjType::Row | ObjType::Column | ObjType::Item) {
            children(g, &mut o, depth + 1, count, target);
        }
        siblings.push(id);
        parent.children.push(o);
    }
}

/// Gives `o` its geometry: what it is placed by, then every coordinate
/// observed, read-only or not.
fn place(g: &mut Gen, o: &mut Obj, parent: &Obj, siblings: &[String], positioner: bool) {
    let anchored = !positioner && g.rng.chance(55);
    let mut set: Vec<&str> = Vec::new();
    if anchored {
        set = anchors(g, o, siblings);
    }
    if positioner && g.rng.chance(15) {
        let e = g.expr(Kind::Bool, 1, &[]);
        g.add(
            o,
            "visible".into(),
            Decl::Builtin { ty: PType::Bool },
            Some(e),
            PType::Bool,
            true,
        );
    }
    let filled = set.contains(&"fill");
    for name in [
        "x",
        "y",
        "width",
        "height",
        "implicitWidth",
        "implicitHeight",
    ] {
        let positioned = matches!(
            (parent.ty, name),
            (ObjType::Row, "x") | (ObjType::Column, "y")
        );
        let held = filled
            || set.contains(&"centerIn") && matches!(name, "x" | "y")
            || name == "x" && set.iter().any(|s| HORIZONTAL.contains(s))
            || name == "y" && set.iter().any(|s| VERTICAL.contains(s))
            || positioned;
        let positioner_size = matches!(o.ty, ObjType::Row | ObjType::Column)
            && matches!(
                name,
                "width" | "height" | "implicitWidth" | "implicitHeight"
            );
        // Mostly nothing writes what something else places; now and then
        // both do, which Qt settles one way.
        // A positioner's implicit size is its own and read-only.
        let readonly = positioner_size && name.starts_with("implicit");
        let write = if readonly {
            false
        } else if held || positioner_size {
            g.rng.chance(5)
        } else if name.starts_with("implicit") {
            g.rng.chance(25)
        } else {
            g.rng.chance(70)
        };
        let init = write.then(|| size(g, parent, name));
        g.add(
            o,
            name.into(),
            Decl::Builtin { ty: PType::Real },
            init,
            PType::Real,
            !readonly,
        );
    }
}

/// A number for `name`: a literal, or a binding to the parent's size or
/// what is already in scope.
fn size(g: &mut Gen, parent: &Obj, name: &str) -> Expr {
    let along = if matches!(name, "x" | "width" | "implicitWidth") {
        "width"
    } else {
        "height"
    };
    match g.rng.below(10) {
        0..4 => g.lit_from(SIZES, Kind::Num),
        4..7 => {
            let r = Expr::Ref {
                obj: parent.id.clone(),
                prop: along.into(),
                kind: Kind::Num,
            };
            let tpl = *g
                .rng
                .pick(&["($0 / 2)", "($0 - 10)", "($0 * 0.3)", "($0 / 3)"]);
            Expr::op("parent-size", tpl, Kind::Num, vec![r])
        }
        _ => g.expr(Kind::Num, 2, &[]),
    }
}

/// Anchors to the parent or a sibling made before; returns the lines set.
fn anchors(g: &mut Gen, o: &mut Obj, siblings: &[String]) -> Vec<&'static str> {
    let to = |g: &mut Gen| -> Expr {
        if !siblings.is_empty() && g.rng.chance(35) {
            let s = g.rng.pick(siblings).clone();
            Expr::Ref {
                obj: s,
                prop: String::new(),
                kind: Kind::Any,
            }
        } else {
            Expr::Lit("parent".into(), Kind::Any)
        }
    };
    let line = |g: &mut Gen, lines: &[&str]| -> Expr {
        let target = to(g);
        let l = *g.rng.pick(lines);
        Expr::op("anchor-line", format!("$0.{l}"), Kind::Any, vec![target])
    };
    let mut set = Vec::new();
    let mut setting = |o: &mut Obj, name: &'static str, e: Expr| {
        set.push(name);
        o.props
            .push(bindings::ast::Prop::setting(format!("anchors.{name}"), e));
    };
    match g.rng.below(10) {
        0..2 => {
            let t = to(g);
            setting(o, "fill", t);
        }
        2..4 => {
            let t = to(g);
            setting(o, "centerIn", t);
        }
        _ => {
            // An edge or two along each axis, of the same axis's lines.
            for axis in [HORIZONTAL, VERTICAL] {
                let mut lines: Vec<&'static str> = axis.to_vec();
                for _ in 0..g.rng.below(3) {
                    let l = lines.remove(g.rng.below(lines.len()));
                    let e = line(g, axis);
                    setting(o, l, e);
                }
            }
        }
    }
    for _ in 0..g.rng.below(3) {
        let m = *g.rng.pick(MARGINS);
        let name = format!("anchors.{m}");
        if o.props.iter().any(|p| p.name == name) {
            continue;
        }
        let e = if g.rng.chance(70) {
            g.lit_from(&["0", "5", "10", "2.5", "-4"], Kind::Num)
        } else {
            g.expr(Kind::Num, 2, &[])
        };
        g.add(
            o,
            name,
            Decl::Builtin { ty: PType::Real },
            Some(e),
            PType::Real,
            true,
        );
    }
    set
}

/// A step that moves something: a size or a margin, the root's size, an
/// anchor let go or moved, or one of the steps of bindings.
fn step(g: &mut Gen, root: &Obj) -> Option<Stmt> {
    match g.rng.below(10) {
        0..4 => {
            let t = g.pick_scope(|a| a.writable && a.ty == PType::Real)?;
            let value = if g.rng.chance(60) {
                g.lit_from(SIZES, Kind::Num)
            } else {
                g.expr(Kind::Num, 2, &[])
            };
            Some(bindings::generate::assign(&t, value))
        }
        4..6 => {
            let prop = *g.rng.pick(&["width", "height"]);
            Some(Stmt::Assign {
                obj: "root".into(),
                prop: prop.into(),
                member: None,
                value: g.lit_from(&["0", "50", "300", "99.5"], Kind::Num),
            })
        }
        6..8 => {
            let lines: Vec<(String, String)> = root
                .all()
                .iter()
                .flat_map(|o| {
                    o.props
                        .iter()
                        .filter(|p| matches!(p.decl, Decl::Setting))
                        .map(|p| (o.id.clone(), p.name.clone()))
                })
                .collect();
            if lines.is_empty() {
                return None;
            }
            let (obj, prop) = g.rng.pick(&lines).clone();
            let edge = prop.trim_start_matches("anchors.");
            let value = if g.rng.chance(60) {
                Expr::Lit("undefined".into(), Kind::Any)
            } else if HORIZONTAL.contains(&edge) {
                Expr::Lit(
                    format!("{obj}.parent.{}", g.rng.pick(HORIZONTAL)),
                    Kind::Any,
                )
            } else if VERTICAL.contains(&edge) {
                Expr::Lit(format!("{obj}.parent.{}", g.rng.pick(VERTICAL)), Kind::Any)
            } else {
                Expr::Lit(format!("{obj}.parent"), Kind::Any)
            };
            Some(Stmt::Assign {
                obj,
                prop,
                member: None,
                value,
            })
        }
        _ => g.step(root),
    }
}

/// As bindings name a difference, and for a coordinate, what places the
/// item: its anchors, its positioner, or its own bindings.
fn describe(p: &Program, d: &Diff) -> (String, String) {
    let (context, class) = bindings::describe(p, d);
    if !matches!(d.kind, DiffKind::Value | DiffKind::Missing) {
        return (context, class);
    }
    let Some((id, _)) = d.key.split_once('.').filter(|_| d.key != "root.log") else {
        return (context, class);
    };
    let parent = p
        .root
        .all()
        .into_iter()
        .find(|o| o.children.iter().any(|c| c.id == id));
    let Some(o) = p.obj(id) else {
        return (context, class);
    };
    let anchor = |name: &str| o.props.iter().any(|p| p.name == format!("anchors.{name}"));
    let placed = if matches!(o.ty, ObjType::Row | ObjType::Column) {
        "positioner"
    } else if parent.is_some_and(|p| matches!(p.ty, ObjType::Row | ObjType::Column)) {
        "in-positioner"
    } else if anchor("fill") {
        "fill"
    } else if anchor("centerIn") {
        "center-in"
    } else if o.props.iter().any(|p| matches!(p.decl, Decl::Setting)) {
        "anchors"
    } else {
        "free"
    };
    (format!("{context}-{placed}"), class)
}
