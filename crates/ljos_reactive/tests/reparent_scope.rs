//! `Graph::reparent_scope`: scopes move between parents in O(1) and are
//! then disposed, frozen and thawed with their new parent.

use std::cell::Cell;
use std::rc::Rc;

use ljos_reactive::{Graph, ReactiveError};

#[test]
fn a_moved_scope_dies_with_its_new_parent() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.create_scope(r).unwrap();
    let b = g.create_scope(r).unwrap();
    let s = g.create_scope(a).unwrap();
    let inner = g.create_scope(s).unwrap();
    let p = g.property(s, 1).unwrap();

    g.reparent_scope(s, b).unwrap();
    g.check_invariants().unwrap();
    assert_eq!(g.scope_parent(s), Some(b));

    g.dispose_scope(a).unwrap();
    assert!(g.scope_alive(s) && g.scope_alive(inner));
    assert_eq!(g.try_get(p), Some(1));

    g.dispose_scope(b).unwrap();
    assert!(!g.scope_alive(s) && !g.scope_alive(inner));
    assert_eq!(g.try_get(p), None);
    g.check_invariants().unwrap();
}

#[test]
fn moving_keeps_both_child_lists_linked() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.create_scope(r).unwrap();
    let b = g.create_scope(r).unwrap();
    let kids: Vec<_> = (0..4).map(|_| g.create_scope(a).unwrap()).collect();
    let other = g.create_scope(b).unwrap();
    // First, middle and last of `a`'s list.
    for k in [kids[0], kids[2], kids[3]] {
        g.reparent_scope(k, b).unwrap();
        g.check_invariants().unwrap();
    }
    // Back to the end of the same parent.
    g.reparent_scope(other, b).unwrap();
    g.check_invariants().unwrap();
    g.reparent_scope(kids[1], b).unwrap();
    g.check_invariants().unwrap();
    // Disposal is newest-first in the new order.
    let order = Rc::new(std::cell::RefCell::new(Vec::new()));
    for (n, k) in [kids[0], kids[2], kids[3], other, kids[1]]
        .into_iter()
        .enumerate()
    {
        let o = order.clone();
        g.on_dispose(k, move || o.borrow_mut().push(n)).unwrap();
    }
    g.dispose_scope(b).unwrap();
    assert_eq!(*order.borrow(), vec![4, 3, 2, 1, 0]);
}

#[test]
fn bad_moves_are_rejected() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.create_scope(r).unwrap();
    let child = g.create_scope(a).unwrap();
    let grandchild = g.create_scope(child).unwrap();
    assert_eq!(g.reparent_scope(r, a), Err(ReactiveError::RootScope));
    assert_eq!(
        g.reparent_scope(a, grandchild),
        Err(ReactiveError::ScopeCycle {
            scope: a,
            parent: grandchild
        })
    );
    assert_eq!(
        g.reparent_scope(a, a),
        Err(ReactiveError::ScopeCycle {
            scope: a,
            parent: a
        })
    );
    let dead = g.create_scope(r).unwrap();
    g.dispose_scope(dead).unwrap();
    assert_eq!(
        g.reparent_scope(dead, a),
        Err(ReactiveError::DeadScope(dead))
    );
    assert_eq!(
        g.reparent_scope(a, dead),
        Err(ReactiveError::DeadScope(dead))
    );
    g.check_invariants().unwrap();
}

#[test]
fn moving_out_of_a_frozen_parent_thaws_parked_effects() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let frozen = g.create_scope(r).unwrap();
    let live = g.create_scope(r).unwrap();
    let s = g.create_scope(frozen).unwrap();
    let src = g.property(r, 0).unwrap();
    let runs = Rc::new(Cell::new(0));
    let n = runs.clone();
    g.effect(s, move |g| {
        g.get(src);
        n.set(n.get() + 1);
    })
    .unwrap();
    assert_eq!(runs.get(), 1);

    g.set_frozen(frozen, true).unwrap();
    g.set(src, 1);
    g.set(src, 2);
    assert_eq!(runs.get(), 1, "parked while frozen");

    g.reparent_scope(s, live).unwrap();
    assert!(!g.is_frozen(s));
    assert_eq!(runs.get(), 2, "runs once on thaw");

    // And freezes when moved under a frozen parent.
    g.reparent_scope(s, frozen).unwrap();
    assert!(g.is_frozen(s));
    g.set(src, 3);
    assert_eq!(runs.get(), 2);
    g.check_invariants().unwrap();
}
