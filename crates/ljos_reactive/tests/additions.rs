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

#[test]
fn modify_changes_a_plain_value_where_it_is_and_notifies() {
    let g: Graph<Vec<i64>> = Graph::new();
    let r = g.root();
    let list = g.property(r, vec![1, 2]).unwrap();
    let total = g
        .binding(r, Vec::new(), move |g| vec![g.get(list).iter().sum()])
        .unwrap();
    let runs = Rc::new(Cell::new(0));
    let seen = runs.clone();
    g.effect(r, move |g| {
        g.get(list);
        seen.set(seen.get() + 1);
    })
    .unwrap();
    assert_eq!((g.get(total), runs.get()), (vec![3], 1));

    assert_eq!(g.modify(list, |v| v.push(4)), Ok(true));
    assert_eq!(g.get(list), vec![1, 2, 4]);
    assert_eq!((g.get(total), runs.get()), (vec![7], 2));
    assert_eq!(g.provenance(list), Some(Provenance::Assigned));

    // Several in a batch are one run of the effect.
    g.batch(|| {
        for x in 5..8 {
            g.modify(list, |v| v.push(x)).unwrap();
        }
    });
    assert_eq!((g.get(list).len(), runs.get()), (6, 3));
    g.check_invariants().unwrap();
}

#[test]
fn modify_leaves_what_is_not_a_plain_value_alone() {
    let g: Graph<Vec<i64>> = Graph::new();
    let r = g.root();
    let a = g.property(r, vec![1]).unwrap();
    let bound = g.binding(r, Vec::new(), move |g| g.get(a)).unwrap();
    // A binding: the caller has to decide what a write means for it.
    assert_eq!(g.modify(bound, |v| v.push(9)), Ok(false));
    assert_eq!(g.get(bound), vec![1]);
    assert!(g.is_bound(bound));

    let fx = g.effect(r, |_| {}).unwrap();
    assert_eq!(
        g.modify(fx, |v| v.push(9)),
        Err(ReactiveError::NotAProperty(fx))
    );
    g.dispose(a).unwrap();
    assert_eq!(g.modify(a, |v| v.push(9)), Err(ReactiveError::DeadNode(a)));
}

#[test]
fn a_quiet_replacement_is_read_later_and_runs_nothing() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let other = g.property(r, 10).unwrap();
    let sum = g.binding(r, 0, move |g| g.get(a) + g.get(other)).unwrap();
    let runs = Rc::new(Cell::new(0));
    let seen = runs.clone();
    g.effect(r, move |g| {
        g.get(a);
        seen.set(seen.get() + 1);
    })
    .unwrap();
    assert_eq!((g.get(sum), runs.get()), (11, 1));

    assert_eq!(g.replace_quietly(a, 5), Ok(true));
    // A new read finds it; what read the old value keeps its result.
    assert_eq!(g.get(a), 5);
    assert_eq!((g.get(sum), runs.get()), (11, 1));
    // Whatever runs again for another reason reads the new value.
    g.set(other, 20);
    assert_eq!((g.get(sum), runs.get()), (25, 1));
    g.check_invariants().unwrap();

    // Not for a binding, an effect or a dead node.
    assert_eq!(g.replace_quietly(sum, 0), Ok(false));
    assert_eq!(g.get(sum), 25);
    let fx = g.effect(r, |_| {}).unwrap();
    assert_eq!(
        g.replace_quietly(fx, 0),
        Err(ReactiveError::NotAProperty(fx))
    );
    let gone = g.property(r, 0).unwrap();
    g.dispose(gone).unwrap();
    assert_eq!(
        g.replace_quietly(gone, 1),
        Err(ReactiveError::DeadNode(gone))
    );
}

#[test]
fn a_layout_write_leaves_the_binding_and_is_an_assignment_without_one() {
    use ljos_reactive::WriteKind;

    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let input = g.property(r, 50).unwrap();
    let width = g.binding(r, 0, move |g| g.get(input)).unwrap();
    let told = Rc::new(RefCell::new(Vec::new()));
    let t = told.clone();
    g.effect(r, move |g| t.borrow_mut().push(g.get(width)))
        .unwrap();

    g.write(width, 80, WriteKind::Layout).unwrap();
    assert_eq!(g.get(width), 80);
    assert!(g.is_bound(width));
    assert_eq!(g.provenance(width), Some(Provenance::Bound));
    // The binding gives the value again when its input changes,
    g.set(input, 60);
    assert_eq!(g.get(width), 60);
    // also a value it had before the layout wrote,
    g.write(width, 80, WriteKind::Layout).unwrap();
    g.set(input, 50);
    g.set(input, 60);
    assert_eq!(g.get(width), 60);
    assert_eq!(*told.borrow(), [50, 80, 60, 80, 50, 60]);
    // and a run that was due when the layout wrote still comes.
    g.batch(|| {
        g.set(input, 70);
        g.write(width, 80, WriteKind::Layout).unwrap();
    });
    assert_eq!(g.get(width), 70);
    g.check_invariants().unwrap();

    // An assignment removes the binding, a layout write after it too.
    let plain = g.binding(r, 0, move |g| g.get(input)).unwrap();
    g.set(plain, 1);
    g.write(plain, 33, WriteKind::Layout).unwrap();
    g.set(input, 90);
    assert_eq!(g.get(plain), 33);
    assert_eq!(g.provenance(plain), Some(Provenance::Assigned));

    // Nothing bound and never written: afterwards it reads as assigned.
    let fresh = g.property(r, 0).unwrap();
    assert_eq!(g.provenance(fresh), Some(Provenance::Default));
    g.write(fresh, 4, WriteKind::Layout).unwrap();
    assert_eq!(g.get(fresh), 4);
    assert_eq!(g.provenance(fresh), Some(Provenance::Assigned));
    g.check_invariants().unwrap();
}

#[test]
fn work_put_off_runs_when_the_computing_node_has_its_value() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let input = g.property(r, 1).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));

    // Nothing computes: it runs where it is asked for.
    let l = log.clone();
    g.after_evaluation(move || l.borrow_mut().push("now".to_string()));
    assert_eq!(*log.borrow(), ["now"]);
    log.borrow_mut().clear();

    // Put off from inside a binding, the work may write that binding's
    // own property, and has run when the read that ran the binding ends.
    let slot: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
    let (l, s, weak) = (log.clone(), slot.clone(), Rc::downgrade(&g));
    let inner = g
        .binding(r, 0, move |g| {
            let v = g.get(input);
            let (later, s, g2) = (l.clone(), s.clone(), weak.upgrade().unwrap());
            g.after_evaluation(move || {
                let n = s.get().unwrap();
                later
                    .borrow_mut()
                    .push(format!("after inner, it is {}", g2.get(n)));
                if v == 2 {
                    g2.write(n, 20, ljos_reactive::WriteKind::Animation)
                        .unwrap();
                }
            });
            l.borrow_mut().push(format!("inner gives {v}"));
            v
        })
        .unwrap();
    slot.set(Some(inner));
    // A binding that reads it is still computing when that work runs.
    let l = log.clone();
    let outer = g
        .binding(r, 0, move |g| {
            let v = g.get(inner);
            l.borrow_mut().push(format!("outer read {v}"));
            v
        })
        .unwrap();
    assert_eq!(g.get(outer), 1);
    assert_eq!(
        *log.borrow(),
        ["inner gives 1", "after inner, it is 1", "outer read 1"]
    );
    log.borrow_mut().clear();

    g.set(input, 2);
    assert_eq!(g.get(inner), 20);
    assert_eq!(*log.borrow(), ["inner gives 2", "after inner, it is 2"]);
    assert!(g.is_bound(inner));
    g.check_invariants().unwrap();
}

#[test]
fn asking_whether_a_property_is_set_is_a_tracked_read() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let margin = g.property(r, 0).unwrap();
    let other = g.property(r, 0).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    g.effect(r, move |g| {
        let v = g.get(margin);
        let set = g.provenance_tracked(margin) != Some(Provenance::Default);
        s.borrow_mut().push((v, set));
    })
    .unwrap();
    // A reader of the value alone is not run for it.
    let runs = Rc::new(Cell::new(0));
    let c = runs.clone();
    g.effect(r, move |g| {
        g.get(other);
        c.set(c.get() + 1);
    })
    .unwrap();

    // The value it had by default, given on purpose: the reader hears.
    g.set(margin, 0);
    assert_eq!(*seen.borrow(), [(0, false), (0, true)]);
    // Given again there is nothing new.
    g.set(margin, 0);
    assert_eq!(seen.borrow().len(), 2);
    g.set(other, 0);
    assert_eq!(runs.get(), 1, "nobody asked about this one");

    // A binding and an override are a value given too.
    for which in 0..2 {
        let p = g.property(r, 7).unwrap();
        let told = Rc::new(RefCell::new(Vec::new()));
        let t = told.clone();
        g.effect(r, move |g| t.borrow_mut().push(g.provenance_tracked(p)))
            .unwrap();
        let after = if which == 0 {
            g.bind(p, |_| 7).unwrap();
            Provenance::Bound
        } else {
            g.push_override(p, OverrideValue::Value(7)).unwrap();
            Provenance::Assigned
        };
        assert_eq!(
            *told.borrow(),
            [Some(Provenance::Default), Some(after)],
            "case {which}"
        );
    }

    // An override that goes leaves the property as unset as it was.
    let p = g.property(r, 7).unwrap();
    let told = Rc::new(RefCell::new(Vec::new()));
    let t = told.clone();
    g.effect(r, move |g| t.borrow_mut().push(g.provenance_tracked(p)))
        .unwrap();
    let id = g.push_override(p, OverrideValue::Value(7)).unwrap();
    g.remove_override(p, id, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(
        *told.borrow(),
        [
            Some(Provenance::Default),
            Some(Provenance::Assigned),
            Some(Provenance::Default)
        ]
    );
    g.check_invariants().unwrap();
}

#[test]
fn a_reset_property_is_unset_again() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let source = g.property(r, 1).unwrap();
    let p = g.property(r, 0).unwrap();
    let told = Rc::new(RefCell::new(Vec::new()));
    let t = told.clone();
    g.effect(r, move |g| {
        t.borrow_mut().push((g.get(p), g.provenance_tracked(p)));
    })
    .unwrap();

    // A given value goes, and with it that the value was given.
    g.set(p, 5);
    g.reset(p, 0).unwrap();
    assert_eq!(g.provenance(p), Some(Provenance::Default));
    assert_eq!(
        *told.borrow(),
        [
            (0, Some(Provenance::Default)),
            (5, Some(Provenance::Assigned)),
            (0, Some(Provenance::Default)),
        ]
    );

    // A binding goes too: its source no longer moves the property.
    g.bind(p, move |g| g.get(source) * 10).unwrap();
    assert_eq!(g.get(p), 10);
    g.reset(p, 0).unwrap();
    assert!(!g.is_bound(p));
    g.set(source, 2);
    assert_eq!(g.get(p), 0);
    assert!(g.sources(p).is_empty());

    // The value it had, given back by a reset: observers hear that it is
    // unset though the value is the same.
    told.borrow_mut().clear();
    g.set(p, 0);
    g.reset(p, 0).unwrap();
    assert_eq!(
        *told.borrow(),
        [
            (0, Some(Provenance::Assigned)),
            (0, Some(Provenance::Default))
        ]
    );

    // Under an override the reset is of the override, as a write is.
    let id = g.push_override(p, OverrideValue::Value(9)).unwrap();
    g.reset(p, 4).unwrap();
    assert_eq!(g.get(p), 4);
    g.remove_override(p, id, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(p), 0);

    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(g.reset(e, 0), Err(ReactiveError::NotAProperty(e)));
    g.check_invariants().unwrap();
}

#[test]
fn settled_reads_what_try_get_does_and_records_nothing() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) * 10).unwrap();
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::new(Cell::new(0));
    let (n, s) = (runs.clone(), seen.clone());
    g.effect(r, move |g| {
        n.set(n.get() + 1);
        s.set(g.settled(a).unwrap() + g.settled(b).unwrap());
    })
    .unwrap();
    assert_eq!(
        (runs.get(), seen.get()),
        (1, 11),
        "a dirty binding is brought up to date"
    );

    // An up-to-date node is a copy of its value: no evaluation, no read.
    let before = g.stats();
    assert_eq!(g.settled(a), Some(1));
    assert_eq!(g.settled(b), Some(10));
    let after = g.stats();
    assert_eq!(after.reads, before.reads);
    assert_eq!(after.evaluations, before.evaluations);

    // The effect read through `settled`, so it follows neither.
    g.set(a, 2);
    assert_eq!(runs.get(), 1);
    assert_eq!(g.settled(b), Some(20));
    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(g.settled(e), None, "an effect has no value");
}
