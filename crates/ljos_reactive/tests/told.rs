//! `Graph::effect_told`: an effect that, after its first run, is told of a
//! change instead of running again.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::{Graph, NodeState};

fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

#[test]
fn the_first_run_is_tracked_and_later_changes_only_tell() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let (runs, tells) = (counter(), counter());
    let (rc, tc) = (runs.clone(), tells.clone());
    let e = g
        .effect_told(
            r,
            move |g| {
                rc.set(rc.get() + 1);
                g.get(a);
            },
            move || tc.set(tc.get() + 1),
        )
        .unwrap();
    assert_eq!((runs.get(), tells.get()), (1, 0));
    assert_eq!(g.sources(e), vec![a]);
    g.set(a, 2);
    g.set(a, 3);
    assert_eq!((runs.get(), tells.get()), (1, 2));
    assert_eq!(g.state(e), Some(NodeState::Clean));
    g.set(a, 3);
    assert_eq!(tells.get(), 2, "no change, no tell");
}

#[test]
fn a_batch_tells_once_when_it_ends() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 1).unwrap();
    let tells = counter();
    let tc = tells.clone();
    g.effect_told(
        r,
        move |g| {
            g.get(a);
            g.get(b);
        },
        move || tc.set(tc.get() + 1),
    )
    .unwrap();
    g.batch(|| {
        g.set(a, 2);
        g.set(b, 2);
        assert_eq!(tells.get(), 0);
    });
    assert_eq!(tells.get(), 1);
}

#[test]
fn a_told_reader_may_use_the_graph() {
    let g = Rc::new(Graph::new());
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) * 10).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (g2, s) = (Rc::downgrade(&g), seen.clone());
    g.effect_told(
        r,
        move |g| {
            g.get(b);
        },
        move || {
            let g = g2.upgrade().unwrap();
            s.borrow_mut().push(g.get_untracked(b));
        },
    )
    .unwrap();
    g.set(a, 2);
    // Read again, so the next change reaches it.
    g.set(a, 3);
    assert_eq!(*seen.borrow(), vec![20, 30]);
}

#[test]
fn a_frozen_scope_runs_it_again_on_thaw() {
    let g = Graph::new();
    let r = g.root();
    let page = g.create_scope(r).unwrap();
    let a = g.property(r, 1).unwrap();
    let (runs, tells) = (counter(), counter());
    let (rc, tc) = (runs.clone(), tells.clone());
    g.effect_told(
        page,
        move |g| {
            rc.set(rc.get() + 1);
            g.get(a);
        },
        move || tc.set(tc.get() + 1),
    )
    .unwrap();
    g.set_frozen(page, true).unwrap();
    g.set(a, 2);
    assert_eq!((runs.get(), tells.get()), (1, 0));
    g.set_frozen(page, false).unwrap();
    assert_eq!((runs.get(), tells.get()), (2, 0));
    g.set(a, 3);
    assert_eq!((runs.get(), tells.get()), (2, 1));
}

#[test]
fn a_source_disposed_runs_it_again() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 5).unwrap();
    let (runs, tells) = (counter(), counter());
    let (rc, tc) = (runs.clone(), tells.clone());
    let e = g
        .effect_told(
            r,
            move |g| {
                rc.set(rc.get() + 1);
                if g.try_get(a).is_none() {
                    g.get(b);
                }
            },
            move || tc.set(tc.get() + 1),
        )
        .unwrap();
    g.dispose(a).unwrap();
    assert_eq!((runs.get(), tells.get()), (2, 0));
    assert_eq!(g.sources(e), vec![b]);
    g.set(b, 6);
    assert_eq!(tells.get(), 1);
    g.check_invariants().unwrap();
}

#[test]
fn disposed_it_is_told_nothing() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let tells = counter();
    let tc = tells.clone();
    let e = g
        .effect_told(
            r,
            move |g| {
                g.get(a);
            },
            move || tc.set(tc.get() + 1),
        )
        .unwrap();
    g.batch(|| {
        g.set(a, 2);
        g.dispose(e).unwrap();
    });
    assert_eq!(tells.get(), 0);
    assert!(g.observers(a).is_empty());
}
