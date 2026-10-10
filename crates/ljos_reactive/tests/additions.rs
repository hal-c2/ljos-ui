//! `is_tracking`, node data, `refresh`, `peek`, `hold` and diagnostic accessors.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::{
    Diagnostic, Graph, NodeId, NodeState, OverrideValue, Provenance, ReactiveError, RestoreMode,
};

#[test]
fn is_tracking_follows_the_evaluation_frame() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    assert!(!g.is_tracking(), "top level records nothing");

    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let a = g.property(r, 1).unwrap();
    let b = g
        .binding(r, 0, move |g| {
            s.borrow_mut().push(("binding", g.is_tracking()));
            let inner = g.untrack(|| g.is_tracking());
            s.borrow_mut().push(("untracked", inner));
            // Tracking resumes once the untracked block ends.
            s.borrow_mut().push(("after", g.is_tracking()));
            g.get(a)
        })
        .unwrap();
    g.get(b);
    let s = seen.clone();
    g.effect(r, move |g| s.borrow_mut().push(("effect", g.is_tracking())))
        .unwrap();
    // `watch` handlers run untracked.
    let s = seen.clone();
    g.watch(r, a, move |g, _| {
        s.borrow_mut().push(("watch", g.is_tracking()));
    })
    .unwrap();
    g.set(a, 2);

    assert_eq!(
        *seen.borrow(),
        vec![
            ("binding", true),
            ("untracked", false),
            ("after", true),
            ("effect", true),
            ("watch", false),
        ]
    );
    assert!(!g.is_tracking());
}

#[test]
fn node_data_round_trips_and_dies_with_the_node() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(g.node_data(p), None);
    g.set_node_data(p, 7).unwrap();
    g.set_node_data(e, u64::MAX).unwrap();
    assert_eq!(g.node_data(p), Some(7));
    assert_eq!(g.node_data(e), Some(u64::MAX));
    g.set_node_data(p, 8).unwrap();
    assert_eq!(g.node_data(p), Some(8));
    g.dispose(p).unwrap();
    assert_eq!(g.node_data(p), None);
    assert_eq!(g.set_node_data(p, 1), Err(ReactiveError::DeadNode(p)));
}

#[test]
fn refresh_reruns_a_binding_and_keeps_provenance() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let outside = Rc::new(Cell::new(10));
    let o = outside.clone();
    let runs = Rc::new(Cell::new(0));
    let rc = runs.clone();
    let b = g
        .binding(r, 0, move |_| {
            rc.set(rc.get() + 1);
            o.get()
        })
        .unwrap();
    assert_eq!(g.get(b), 10);
    assert_eq!(g.provenance(b), Some(Provenance::Bound));

    // The closure reads something the graph cannot see: only refresh helps.
    outside.set(20);
    assert_eq!(g.get(b), 10);
    g.refresh(b).unwrap();
    assert_eq!(g.state(b), Some(NodeState::Dirty), "lazy until read");
    assert_eq!(g.get(b), 20);
    assert_eq!(runs.get(), 2);
    assert_eq!(g.provenance(b), Some(Provenance::Bound));
    assert!(g.is_bound(b));
}

#[test]
fn refresh_notifies_observers_and_cuts_off_on_equal_values() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let outside = Rc::new(Cell::new(1));
    let o = outside.clone();
    let b = g.binding(r, 0, move |_| o.get()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    g.watch(r, b, move |_, v| s.borrow_mut().push(*v)).unwrap();

    g.refresh(b).unwrap();
    assert!(seen.borrow().is_empty(), "equal value stops the wave");
    outside.set(2);
    g.refresh(b).unwrap();
    assert_eq!(*seen.borrow(), vec![2]);
}

#[test]
fn refresh_keeps_the_override_stack() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let outside = Rc::new(Cell::new(1));
    let o = outside.clone();
    let base = g.binding(r, 0, move |_| o.get()).unwrap();
    let o = outside.clone();
    let id = g
        .push_override(base, OverrideValue::binding(move |_| o.get() * 100))
        .unwrap();
    assert_eq!(g.get(base), 100);

    outside.set(2);
    g.refresh(base).unwrap();
    assert_eq!(g.get(base), 200, "the override is the active layer");
    assert_eq!(g.override_count(base), 1);
    assert_eq!(g.provenance(base), Some(Provenance::Bound));

    g.remove_override(base, id, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(base), 2, "the base binding is still installed");
}

#[test]
fn refresh_on_a_value_or_dead_node() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let p = g.property(r, 3).unwrap();
    g.set(p, 4);
    g.refresh(p).unwrap();
    assert_eq!(g.get(p), 4);
    assert_eq!(g.provenance(p), Some(Provenance::Assigned));
    g.dispose(p).unwrap();
    assert_eq!(g.refresh(p), Err(ReactiveError::DeadNode(p)));
}

#[test]
fn refresh_runs_an_eager_binding_and_an_effect_now() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let outside = Rc::new(Cell::new(1));
    let o = outside.clone();
    let b = g.binding(r, 0, move |_| o.get()).unwrap();
    g.set_eager(b, true).unwrap();
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    let e = g.effect(r, move |_| h.set(h.get() + 1)).unwrap();
    assert_eq!(hits.get(), 1);

    outside.set(5);
    g.refresh(b).unwrap();
    assert_eq!(g.state(b), Some(NodeState::Clean), "eager roots flush");
    g.refresh(e).unwrap();
    assert_eq!(hits.get(), 2);
    // Batched: one run however often it is refreshed.
    g.batch(|| {
        g.refresh(e).unwrap();
        g.refresh(e).unwrap();
    });
    assert_eq!(hits.get(), 3);
}

#[test]
fn refresh_while_computing_runs_once_more() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let runs = Rc::new(Cell::new(0));
    let rc = runs.clone();
    let slot = Rc::new(Cell::new(None));
    let sl = slot.clone();
    let b = g
        .binding(r, 0, move |g| {
            rc.set(rc.get() + 1);
            if rc.get() == 1
                && let Some(me) = sl.get()
            {
                g.refresh(me).unwrap();
            }
            i64::from(rc.get())
        })
        .unwrap();
    slot.set(Some(b));
    g.set_eager(b, true).unwrap();
    // The first run asked for another one, which a later read observes.
    assert_eq!(g.get(b), 2);
    assert_eq!(runs.get(), 2);
}

/// Two properties that bind to each other, tagged 100 and 200.
fn loop_pair(g: &Graph<i64>) -> (NodeId, NodeId) {
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    g.set_node_data(a, 100).unwrap();
    g.set_node_data(b, 200).unwrap();
    g.bind(a, move |g| g.get(b) + 1).unwrap();
    g.bind(b, move |g| g.get(a) + 1).unwrap();
    (a, b)
}

#[test]
fn binding_loop_diagnostic_names_the_property_through_node_data() {
    let g: Graph<i64> = Graph::new();
    let (a, _) = loop_pair(&g);
    g.get(a);
    let diags = g.take_diagnostics();
    let [d @ Diagnostic::BindingLoop { .. }] = &diags[..] else {
        panic!("expected one loop, got {diags:?}");
    };
    assert_eq!(d.node(), a);
    assert_eq!(g.node_data(d.node()), Some(100));
    let cycle: Vec<_> = d.cycle().iter().map(|n| g.node_data(*n)).collect();
    assert_eq!(cycle, vec![Some(100), Some(200)]);
}

#[test]
fn a_diagnostic_handler_can_read_node_data() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let tags = Rc::new(RefCell::new(Vec::new()));
    let (gh, t) = (g.clone(), tags.clone());
    g.set_diagnostic_handler(move |d| t.borrow_mut().push(gh.node_data(d.node())));
    let (a, _) = loop_pair(&g);
    g.get(a);
    assert_eq!(*tags.borrow(), vec![Some(100)]);
}

#[test]
fn rerun_limit_diagnostic_carries_the_node() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.property(r, 0).unwrap();
    let wa = g.watch(r, a, move |g, v| g.set(b, v + 1)).unwrap();
    g.set_node_data(wa, 42).unwrap();
    let wb = g.watch(r, b, move |g, v| g.set(a, v + 1)).unwrap();
    g.set_node_data(wb, 43).unwrap();
    g.set(a, 1);
    let diags = g.take_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| matches!(d, Diagnostic::RerunLimit { .. })
                && matches!(g.node_data(d.node()), Some(42 | 43))
                && d.cycle().is_empty())
    );
}

#[test]
fn peek_runs_nothing_and_records_nothing() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let runs = Rc::new(Cell::new(0));
    let c = runs.clone();
    let b = g
        .binding(r, 7, move |g| {
            c.set(c.get() + 1);
            g.get(a) + 1
        })
        .unwrap();
    assert_eq!(g.peek(b), Some(7), "the value it was installed over");
    assert_eq!(runs.get(), 0);
    assert_eq!(g.state(b), Some(NodeState::Dirty));

    let reader = g.binding(r, 0, move |g| g.peek(b).unwrap_or(0)).unwrap();
    assert_eq!(g.get(reader), 7);
    assert!(g.sources(reader).is_empty(), "a peek is not a dependency");

    assert_eq!(g.get(b), 2);
    assert_eq!(g.peek(b), Some(2));
    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(g.peek(e), None);
    g.dispose(b).unwrap();
    assert_eq!(g.peek(b), None);
}

#[test]
fn a_held_binding_reads_as_the_old_value_until_it_is_released() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let runs = Rc::new(Cell::new(0));
    let c = runs.clone();
    let b = g
        .binding(r, 7, move |g| {
            c.set(c.get() + 1);
            g.get(a) + 1
        })
        .unwrap();
    g.hold(b).unwrap();
    assert!(g.is_held(b));
    assert!(g.is_bound(b));

    let reader = g.binding(r, 0, move |g| g.get(b) * 10).unwrap();
    assert_eq!(g.get(reader), 70);
    assert_eq!(runs.get(), 0, "reading a held binding does not run it");
    g.set(a, 5);
    assert_eq!(g.get(reader), 70, "it has no sources yet");

    g.release(b).unwrap();
    assert!(!g.is_held(b));
    assert_eq!(g.get(reader), 60, "what read it runs again");
    assert_eq!(runs.get(), 1);
    g.check_invariants().unwrap();
}

#[test]
fn releasing_runs_an_eager_reader_now() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let b = g.binding(r, 7, |_| 3).unwrap();
    g.hold(b).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    g.effect(r, move |g| s.borrow_mut().push(g.get(b))).unwrap();
    assert_eq!(*seen.borrow(), vec![7]);
    g.release(b).unwrap();
    assert_eq!(*seen.borrow(), vec![7, 3]);
}

#[test]
fn a_write_or_a_new_binding_ends_a_hold() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let b = g.binding(r, 7, |_| 3).unwrap();
    g.hold(b).unwrap();
    g.set(b, 9);
    assert!(!g.is_held(b));
    g.release(b).unwrap();
    assert_eq!(g.get(b), 9, "the binding that was held is gone");

    g.bind(b, |_| 4).unwrap();
    g.hold(b).unwrap();
    g.bind(b, |_| 5).unwrap();
    assert!(!g.is_held(b), "the new binding is not held");
    assert_eq!(g.get(b), 5);
}

#[test]
fn hold_only_takes_a_binding_that_has_not_run() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let p = g.property(r, 1).unwrap();
    g.hold(p).unwrap();
    assert!(!g.is_held(p), "a plain value has nothing to hold");

    let b = g.binding(r, 0, move |g| g.get(p) + 1).unwrap();
    assert_eq!(g.get(b), 2);
    g.hold(b).unwrap();
    assert!(!g.is_held(b), "it is clean: there is nothing to hold back");

    let e = g.effect(r, |_| {}).unwrap();
    assert!(matches!(g.hold(e), Err(ReactiveError::NotAProperty(_))));
    g.dispose(b).unwrap();
    assert!(matches!(g.hold(b), Err(ReactiveError::DeadNode(_))));
    assert!(matches!(g.release(b), Err(ReactiveError::DeadNode(_))));
    assert!(!g.is_held(b));
}
