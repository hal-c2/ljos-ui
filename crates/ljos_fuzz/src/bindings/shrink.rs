//! The smaller programs a program can become.
//!
//! Each candidate takes one thing away (an object, a step, a property, a
//! handler) or makes one expression simpler, and then repairs what pointed
//! at what was taken: a reference becomes a literal of its kind, a write to
//! a property that is gone is dropped, an alias of it goes too. Candidates
//! that take away most come first, so the greedy shrinker in
//! [`crate::shrink`] gets small quickly.

use std::collections::BTreeSet;

use super::ast::{Decl, Expr, Program, Stmt};

/// Candidates, largest step first.
#[must_use]
pub fn reductions(p: &Program) -> Vec<Program> {
    let mut out = Vec::new();
    let mut push = |mut c: Program| {
        repair(&mut c);
        if c != *p {
            out.push(c);
        }
    };
    let ids: Vec<String> = p.root.all().iter().skip(1).map(|o| o.id.clone()).collect();
    for id in &ids {
        let mut c = p.clone();
        c.root.each_mut(&mut |o| o.children.retain(|k| k.id != *id));
        push(c);
    }
    if !p.steps.is_empty() {
        let mut c = p.clone();
        c.steps.clear();
        push(c);
    }
    for i in (0..p.steps.len()).rev() {
        let mut c = p.clone();
        c.steps.remove(i);
        push(c);
    }
    let all: Vec<(String, usize)> = p
        .root
        .all()
        .iter()
        .flat_map(|o| (0..o.props.len()).map(|i| (o.id.clone(), i)))
        .collect();
    for (id, i) in &all {
        push(on(p, id, |o| {
            o.props.remove(*i);
        }));
    }
    for o in p.root.all() {
        let id = o.id.clone();
        for i in 0..o.funcs.len() {
            push(on(p, &id, |o| {
                o.funcs.remove(i);
            }));
        }
        for i in 0..o.signals.len() {
            push(on(p, &id, |o| {
                o.signals.remove(i);
            }));
            push(on(p, &id, |o| o.signals[i].handler = None));
        }
        for i in 0..o.changed.len() {
            push(on(p, &id, |o| {
                o.changed.remove(i);
            }));
        }
        if o.completed.is_some() {
            push(on(p, &id, |o| o.completed = None));
        }
        let bodies = {
            let mut copy = o.clone();
            copy.bodies_mut()
                .iter()
                .map(|b| b.len())
                .collect::<Vec<_>>()
        };
        for (b, len) in bodies.iter().enumerate() {
            for s in 0..*len {
                push(on(p, &id, |o| {
                    o.bodies_mut()[b].remove(s);
                }));
            }
        }
        for (i, prop) in o.props.iter().enumerate() {
            if prop.init.is_some() && !prop.readonly() && !matches!(prop.decl, Decl::Alias { .. }) {
                push(on(p, &id, |o| o.props[i].init = None));
            }
        }
        for (i, prop) in o.props.iter().enumerate() {
            if prop.readonly() {
                push(on(p, &id, |o| {
                    if let Decl::New { readonly, .. } = &mut o.props[i].decl {
                        *readonly = false;
                    }
                }));
            }
        }
        for (i, f) in o.funcs.iter().enumerate() {
            if f.throws.is_some() {
                push(on(p, &id, |o| o.funcs[i].throws = None));
            }
        }
    }
    if !p.enums.is_empty() {
        let mut c = p.clone();
        c.enums.clear();
        push(c);
    }
    let mut count = 0;
    let mut copy = p.clone();
    copy.exprs_mut(&mut |e| e.visit(&mut |_| count += 1));
    for at in 0..count {
        let Some(node) = nth(p, at) else { continue };
        let kind = node.kind();
        if let Expr::Op { args, .. } = &node {
            for a in args
                .iter()
                .filter(|a| a.kind() == kind || kind == super::ast::Kind::Any)
            {
                push(replace(p, at, a.clone()));
            }
        }
        let lit = kind.literal();
        if node != lit {
            push(replace(p, at, lit));
        }
    }
    out
}

/// The program with `f` done to the object `id`.
fn on(p: &Program, id: &str, f: impl FnOnce(&mut super::ast::Obj)) -> Program {
    let mut c = p.clone();
    let mut f = Some(f);
    c.root.each_mut(&mut |o| {
        if o.id == id
            && let Some(f) = f.take()
        {
            f(o);
        }
    });
    c
}

/// The `at`th expression node, counting depth first over the program.
fn nth(p: &Program, at: usize) -> Option<Expr> {
    let mut i = 0;
    let mut found = None;
    let mut copy = p.clone();
    copy.exprs_mut(&mut |e| {
        e.visit(&mut |n| {
            if i == at {
                found = Some(n.clone());
            }
            i += 1;
        });
    });
    found
}

fn replace(p: &Program, at: usize, with: Expr) -> Program {
    let mut c = p.clone();
    let mut i = 0;
    let mut with = Some(with);
    c.exprs_mut(&mut |e| {
        e.visit_mut(&mut |n| {
            if i == at
                && let Some(w) = with.take()
            {
                *n = w;
            }
            i += 1;
        });
    });
    c
}

/// Makes everything that points at something no longer there point at
/// nothing: what is left is a program Qt still loads.
pub fn repair(p: &mut Program) {
    loop {
        let props = existing(p);
        let mut gone = false;
        p.root.each_mut(&mut |o| {
            let before = o.props.len();
            o.props.retain(|prop| match &prop.decl {
                Decl::Alias { target, .. } => {
                    let mut parts = target.splitn(3, '.');
                    let (obj, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    props.contains(&(obj.to_owned(), name.to_owned()))
                }
                _ => true,
            });
            gone |= o.props.len() != before;
        });
        if !gone {
            break;
        }
    }
    let props = existing(p);
    let objs: BTreeSet<String> = p.root.all().iter().map(|o| o.id.clone()).collect();
    let funcs: BTreeSet<(String, String)> = p
        .root
        .all()
        .iter()
        .flat_map(|o| o.funcs.iter().map(|f| (o.id.clone(), f.name.clone())))
        .collect();
    let signals: BTreeSet<(String, String)> = p
        .root
        .all()
        .iter()
        .flat_map(|o| o.signals.iter().map(|s| (o.id.clone(), s.name.clone())))
        .collect();
    let has_enums = !p.enums.is_empty();
    let fix = &mut |e: &mut Expr| {
        e.visit_mut(&mut |n| {
            let dead = match n {
                Expr::Ref { obj, prop, .. } if prop.is_empty() => !objs.contains(obj),
                Expr::Ref { obj, prop, .. } => !props.contains(&(obj.clone(), prop.clone())),
                Expr::Op {
                    label: "function-call",
                    tpl,
                    ..
                } => {
                    let callee = tpl.split_once('(').map_or("", |(c, _)| c);
                    let (obj, f) = callee.split_once('.').unwrap_or(("", ""));
                    !funcs.contains(&(obj.to_owned(), f.to_owned()))
                }
                Expr::Lit(t, _) => !has_enums && t.starts_with("Case."),
                _ => false,
            };
            if dead {
                *n = n.kind().literal();
            }
        });
    };
    p.exprs_mut(fix);
    let alive = |s: &Stmt| match s {
        Stmt::Assign { obj, prop, .. }
        | Stmt::Bind { obj, prop, .. }
        | Stmt::Push { obj, prop, .. } => props.contains(&(obj.clone(), prop.clone())),
        Stmt::Call { obj, fun, .. } => funcs.contains(&(obj.clone(), fun.clone())),
        Stmt::Emit { obj, sig, .. } => signals.contains(&(obj.clone(), sig.clone())),
        Stmt::Log { .. } => true,
    };
    p.steps.retain(alive);
    p.root.each_mut(&mut |o| {
        for body in o.bodies_mut() {
            body.retain(alive);
        }
        let own: BTreeSet<&str> = o.props.iter().map(|p| p.name.as_str()).collect();
        let keep: Vec<bool> = o
            .changed
            .iter()
            .map(|c| own.contains(c.prop.as_str()))
            .collect();
        let mut k = keep.into_iter();
        o.changed.retain(|_| k.next().unwrap_or(true));
    });
}

fn existing(p: &Program) -> BTreeSet<(String, String)> {
    p.root
        .all()
        .iter()
        .flat_map(|o| o.props.iter().map(|p| (o.id.clone(), p.name.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bindings::generate, rng::Rng};

    #[test]
    fn every_candidate_is_smaller_and_whole() {
        for n in 0..40 {
            let p = generate::program(&mut Rng::for_case(5, n));
            for c in reductions(&p).into_iter().take(60) {
                assert!(c.size() <= p.size(), "case {n}");
                let mut again = c.clone();
                repair(&mut again);
                assert_eq!(again, c, "a candidate is already repaired");
            }
        }
    }

    #[test]
    fn a_removed_property_leaves_no_reference_behind() {
        let mut p = generate::program(&mut Rng::for_case(2, 3));
        p.root.each_mut(&mut |o| o.props.clear());
        repair(&mut p);
        p.exprs_mut(&mut |e| {
            e.visit(&mut |n| {
                assert!(!matches!(n, Expr::Ref { prop, .. } if !prop.is_empty()));
            });
        });
        assert!(p.steps.iter().all(|s| s.target().is_none()));
    }
}
