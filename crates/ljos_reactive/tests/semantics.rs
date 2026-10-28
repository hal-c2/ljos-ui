//! One test per rule in ARCHITECTURE.md §6.2 and §8.3.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::{
    Diagnostic, Graph, NodeId, NodeState, Options, OverrideValue, Provenance, ReactiveError,
    RestoreMode, WriteKind,
};

type Log<T> = Rc<RefCell<Vec<T>>>;

fn log<T>() -> Log<T> {
    Rc::new(RefCell::new(Vec::new()))
}

fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

fn counted_binding(
    g: &Graph<i64>,
    count: &Rc<Cell<u32>>,
    f: impl Fn(&Graph<i64>) -> i64 + 'static,
) -> NodeId {
    let c = count.clone();
    g.binding(g.root(), 0, move |g| {
        c.set(c.get() + 1);
        f(g)
    })
    .unwrap()
}

#[test]
fn bindings_are_lazy_and_cached() {
    let g = Graph::new();
    let a = g.property(g.root(), 1).unwrap();
    let n = counter();
    let b = counted_binding(&g, &n, move |g| g.get(a) + 1);
    assert_eq!(n.get(), 0, "not evaluated until read");
    assert_eq!(g.state(b), Some(NodeState::Dirty));
    assert_eq!(g.get(b), 2);
    assert_eq!(g.get(b), 2);
    assert_eq!(n.get(), 1, "cached");
    g.set(a, 5);
    assert_eq!(n.get(), 1, "a write does not evaluate a lazy binding");
    assert_eq!(g.get(b), 6);
    assert_eq!(n.get(), 2);
}

#[test]
fn diamond_is_glitch_free_and_runs_effect_once() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) * 2).unwrap();
    let c = g.binding(r, 0, move |g| g.get(a) * 3).unwrap();
    let seen = log();
    let s = seen.clone();
    g.effect(r, move |g| s.borrow_mut().push((g.get(b), g.get(c))))
        .unwrap();
    g.set(a, 2);
    assert_eq!(*seen.borrow(), vec![(2, 3), (4, 6)]);
}

#[test]
fn equal_values_do_not_propagate() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let parity = g.binding(r, 0, move |g| g.get(a) % 2).unwrap();
    let n = counter();
    let down = counted_binding(&g, &n, move |g| g.get(parity) + 100);
    let runs = counter();
    let rc = runs.clone();
    g.effect(r, move |g| {
        g.get(down);
        rc.set(rc.get() + 1);
    })
    .unwrap();
    assert_eq!((n.get(), runs.get()), (1, 1));
    g.set(a, 3); // parity unchanged
    assert_eq!((n.get(), runs.get()), (1, 1), "cut off at parity");
    let before = g.stats().propagations;
    g.set(a, 3); // same value
    assert_eq!(g.stats().propagations, before);
    g.set(a, 4);
    assert_eq!((n.get(), runs.get()), (2, 2));
}

#[test]
fn custom_equality_is_used() {
    // Compare modulo 10: 3 -> 13 is "no change".
    let g: Graph<i64> = Graph::with_options(Options {
        equals: |a, b| a % 10 == b % 10,
        ..Options::default()
    });
    let a = g.property(g.root(), 3).unwrap();
    let hits = counter();
    let h = hits.clone();
    g.watch(g.root(), a, move |_, _| h.set(h.get() + 1))
        .unwrap();
    g.set(a, 13);
    assert_eq!(hits.get(), 0);
    assert_eq!(g.get(a), 3, "equal write is not stored");
    g.set(a, 14);
    assert_eq!(hits.get(), 1);
}

#[test]
fn notify_always_propagates() {
    let g = Graph::new();
    let a = g.property(g.root(), 1).unwrap();
    let hits = log();
    let h = hits.clone();
    g.watch(g.root(), a, move |_, v| h.borrow_mut().push(*v))
        .unwrap();
    g.notify(a).unwrap();
    g.notify(a).unwrap();
    assert_eq!(*hits.borrow(), vec![1, 1]);
}

#[test]
fn external_revision_counter_reevaluates_only_its_readers() {
    // qsTr reads I18n::revision; bumping it re-runs translated bindings only.
    let g = Graph::new();
    let r = g.root();
    let revision = g.property(r, 0).unwrap();
    let other = g.property(r, 0).unwrap();
    let tr = counter();
    let plain = counter();
    let translated = counted_binding(&g, &tr, move |g| g.get(revision));
    let untranslated = counted_binding(&g, &plain, move |g| g.get(other));
    g.get(translated);
    g.get(untranslated);
    g.notify(revision).unwrap();
    g.get(translated);
    g.get(untranslated);
    assert_eq!((tr.get(), plain.get()), (2, 1));
}

#[test]
fn unbatched_writes_notify_per_write_batched_once() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.property(r, 0).unwrap();
    let c = g.binding(r, 0, move |g| g.get(a) + g.get(b)).unwrap();
    let seen = log();
    let s = seen.clone();
    g.watch(r, c, move |_, v| s.borrow_mut().push(*v)).unwrap();

    // Qt quirk apps rely on: one handler call per intermediate value.
    g.set(a, 1);
    g.set(b, 2);
    assert_eq!(*seen.borrow(), vec![1, 3]);

    let flushes = g.stats().flushes;
    g.batch(|| {
        g.set(a, 10);
        g.set(b, 20);
        // Reads inside a batch pull current values.
        assert_eq!(g.get(c), 30);
        g.batch(|| g.set(a, 11));
    });
    assert_eq!(*seen.borrow(), vec![1, 3, 31]);
    assert_eq!(g.stats().flushes, flushes + 1);
}

#[test]
fn handlers_are_synchronous_guard_flag_idiom() {
    let g = Graph::new();
    let r = g.root();
    let x = g.property(r, 0).unwrap();
    let guard = Rc::new(Cell::new(false));
    let fired = log();
    let (gd, f) = (guard.clone(), fired.clone());
    g.watch(r, x, move |_, v| {
        if !gd.get() {
            f.borrow_mut().push(*v);
        }
    })
    .unwrap();
    guard.set(true);
    g.set(x, 1);
    guard.set(false);
    g.set(x, 2);
    assert_eq!(*fired.borrow(), vec![2]);
}

#[test]
fn effects_run_in_height_order() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
    let c = g.binding(r, 0, move |g| g.get(b) + 1).unwrap();
    let order = log();
    // Created deepest first so creation order cannot explain the result.
    for (name, node) in [("c", c), ("b", b), ("a", a)] {
        let o = order.clone();
        g.effect(r, move |g| {
            g.get(node);
            o.borrow_mut().push(name);
        })
        .unwrap();
    }
    order.borrow_mut().clear();
    g.set(a, 5);
    assert_eq!(*order.borrow(), vec!["a", "b", "c"]);
}

#[test]
fn dependencies_follow_the_branch_taken() {
    let g = Graph::new();
    let r = g.root();
    let flag = g.property(r, 1).unwrap();
    let left = g.property(r, 10).unwrap();
    let right = g.property(r, 20).unwrap();
    let n = counter();
    let pick = counted_binding(&g, &n, move |g| {
        if g.get(flag) != 0 {
            g.get(left)
        } else {
            g.get(right)
        }
    });
    assert_eq!(g.get(pick), 10);
    assert_eq!(g.sources(pick), vec![flag, left]);
    g.set(right, 21);
    assert_eq!(
        g.state(pick),
        Some(NodeState::Clean),
        "right not a dependency"
    );
    g.set(flag, 0);
    assert_eq!(g.get(pick), 21);
    assert_eq!(g.sources(pick), vec![flag, right]);
    assert!(g.observers(left).is_empty(), "dead branch unsubscribed");
    g.set(left, 11);
    assert_eq!(g.state(pick), Some(NodeState::Clean));
}

#[test]
fn binding_loop_is_reported_and_broken() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    g.bind(a, move |g| g.get(b) + 1).unwrap();
    g.bind(b, move |g| g.get(a) + 1).unwrap();
    // Reading a evaluates b, which reads a while a is computing.
    let v = g.get(a);
    assert_eq!(v, 3, "b saw a's stale value 1");
    let diags = g.take_diagnostics();
    assert_eq!(
        diags,
        vec![Diagnostic::BindingLoop {
            node: a,
            cycle: vec![a, b]
        }]
    );
    // Further reads terminate too.
    g.get(b);
    g.get(a);
}

#[test]
fn self_reading_binding_is_a_loop() {
    let g = Graph::new();
    let a = g.property(g.root(), 5).unwrap();
    g.bind(a, move |g| g.get(a) + 1).unwrap();
    assert_eq!(g.get(a), 6);
    assert!(matches!(
        g.take_diagnostics()[..],
        [Diagnostic::BindingLoop { node, .. }] if node == a
    ));
    assert!(g.sources(a).is_empty(), "no self edge");
}

#[test]
fn handler_ping_pong_is_capped() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.property(r, 0).unwrap();
    g.watch(r, a, move |g, v| g.set(b, v + 1)).unwrap();
    g.watch(r, b, move |g, v| g.set(a, v + 1)).unwrap();
    g.set(a, 1);
    let diags = g.take_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| matches!(d, Diagnostic::RerunLimit { runs: 100, .. })),
        "{diags:?}"
    );
    // The graph still works afterwards.
    g.set(a, 1000);
    assert!(g.get(a) >= 1000);
}

#[test]
fn diagnostic_handler_receives_reports() {
    let g = Graph::new();
    let got = log();
    let s = got.clone();
    g.set_diagnostic_handler(move |d| s.borrow_mut().push(d.clone()));
    let a = g.property(g.root(), 0).unwrap();
    g.bind(a, move |g| g.get(a) + 1).unwrap();
    g.get(a);
    assert_eq!(got.borrow().len(), 1);
    assert!(g.take_diagnostics().is_empty());
}

#[test]
fn assignment_breaks_binding_and_qt_binding_reinstalls() {
    let g = Graph::new();
    let r = g.root();
    let src = g.property(r, 1).unwrap();
    let x = g.property(r, 0).unwrap();
    assert_eq!(g.provenance(x), Some(Provenance::Default));
    g.bind(x, move |g| g.get(src) * 10).unwrap();
    assert_eq!(g.get(x), 10);
    assert_eq!(g.provenance(x), Some(Provenance::Bound));

    g.set(x, 7);
    assert_eq!(g.provenance(x), Some(Provenance::Assigned));
    assert!(!g.is_bound(x));
    assert!(g.observers(src).is_empty());
    g.set(src, 2);
    assert_eq!(g.get(x), 7, "binding is gone");

    g.bind(x, move |g| g.get(src) * 100).unwrap();
    assert_eq!(g.get(x), 200);
    assert_eq!(g.base_provenance(x), Some(Provenance::Bound));
}

#[test]
fn animation_write_keeps_binding() {
    let g = Graph::new();
    let r = g.root();
    let target = g.property(r, 100).unwrap();
    let x = g.binding(r, 0, move |g| g.get(target)).unwrap();
    assert_eq!(g.get(x), 100);
    g.write(x, 50, WriteKind::Animation).unwrap();
    assert_eq!(g.get(x), 50);
    assert_eq!(g.provenance(x), Some(Provenance::Animated));
    assert!(g.is_bound(x));
    g.set(target, 200);
    assert_eq!(g.get(x), 200);
    assert_eq!(g.provenance(x), Some(Provenance::Bound));
}

#[test]
fn host_write_keeps_binding_and_says_it_was_the_host() {
    let g = Graph::new();
    let r = g.root();
    let target = g.property(r, 100).unwrap();
    let x = g.binding(r, 0, move |g| g.get(target)).unwrap();
    assert_eq!(g.get(x), 100);
    g.write(x, 50, WriteKind::Host).unwrap();
    assert_eq!(g.get(x), 50);
    assert_eq!(g.provenance(x), Some(Provenance::Host));
    g.write(x, 60, WriteKind::Animation).unwrap();
    assert_eq!(g.provenance(x), Some(Provenance::Animated));
    g.write(x, 70, WriteKind::Host).unwrap();
    g.set(target, 200);
    assert_eq!(g.get(x), 200);
    assert_eq!(g.provenance(x), Some(Provenance::Bound));
}

#[test]
fn override_stack_restore_modes() {
    let g = Graph::new();
    let r = g.root();
    let src = g.property(r, 1).unwrap();
    let x = g.binding(r, 0, move |g| g.get(src)).unwrap();
    let seen = log();
    let s = seen.clone();
    g.watch(r, x, move |_, v| s.borrow_mut().push(*v)).unwrap();

    // State A: constant override.
    let a = g.push_override(x, OverrideValue::Value(5)).unwrap();
    assert_eq!(g.get(x), 5);
    assert_eq!(g.override_count(x), 1);
    g.set(src, 2);
    assert_eq!(g.get(x), 5, "covered binding is inactive");

    // Binding {} on top of the state.
    let b = g
        .push_override(x, OverrideValue::binding(move |g| g.get(src) * 100))
        .unwrap();
    assert_eq!(g.get(x), 200);
    assert_eq!(g.provenance(x), Some(Provenance::Bound));

    // Removing the covered state just drops it.
    g.remove_override(x, a, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 200);

    // Restoring the base binding.
    g.remove_override(x, b, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 2);
    assert_eq!(g.override_count(x), 0);
    g.set(src, 3);
    assert_eq!(g.get(x), 3, "base binding live again");
    assert_eq!(*seen.borrow(), vec![5, 200, 2, 3]);

    // RestoreMode::None keeps the current value and breaks the base binding.
    let c = g.push_override(x, OverrideValue::Value(9)).unwrap();
    g.remove_override(x, c, RestoreMode::None).unwrap();
    assert_eq!(g.get(x), 9);
    assert!(!g.is_bound(x));
    assert_eq!(g.provenance(x), Some(Provenance::Assigned));

    // RestoreMode::Value restores a plain value...
    let d = g.push_override(x, OverrideValue::Value(1)).unwrap();
    g.remove_override(x, d, RestoreMode::Value).unwrap();
    assert_eq!(g.get(x), 9);
    // ...but RestoreMode::Binding does not.
    let e = g.push_override(x, OverrideValue::Value(4)).unwrap();
    g.remove_override(x, e, RestoreMode::Binding).unwrap();
    assert_eq!(g.get(x), 4);

    assert_eq!(
        g.remove_override(x, e, RestoreMode::None),
        Err(ReactiveError::UnknownOverride(x, e))
    );
}

#[test]
fn a_write_under_a_cover_is_kept_below_it_and_seen_once_it_comes_off() {
    let g = Graph::new();
    let r = g.root();
    let src = g.property(r, 1).unwrap();
    let x = g.binding(r, 0, move |g| g.get(src)).unwrap();
    let seen = log();
    let s = seen.clone();
    g.watch(r, x, move |_, v| s.borrow_mut().push(*v)).unwrap();
    assert_eq!(g.get(x), 1);

    let c = g.push_cover(x, OverrideValue::binding(|_| 0)).unwrap();
    assert_eq!(g.get(x), 0);
    g.set(x, 7);
    assert_eq!(g.get(x), 0, "the write went under the cover");
    assert_eq!(g.provenance(x), Some(Provenance::Bound));
    assert_eq!(g.base_provenance(x), Some(Provenance::Assigned));
    g.set(src, 2);
    assert_eq!(g.get(x), 0);

    g.remove_override(x, c, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 7, "the write broke the base binding");
    g.set(src, 3);
    assert_eq!(g.get(x), 7);
    assert_eq!(*seen.borrow(), vec![0, 7]);

    // An override above the cover takes writes as before.
    let c = g.push_cover(x, OverrideValue::binding(|_| 0)).unwrap();
    let o = g.push_override(x, OverrideValue::Value(5)).unwrap();
    g.set(x, 6);
    assert_eq!(g.get(x), 6);
    g.remove_override(x, o, RestoreMode::BindingOrValue)
        .unwrap();
    g.remove_override(x, c, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 7);
}

#[test]
fn a_write_under_nested_covers_survives_removing_the_lower_cover() {
    let g = Graph::new();
    let r = g.root();
    let x = g.property(r, 0).unwrap();
    let lower = g.push_cover(x, OverrideValue::Value(0)).unwrap();
    let upper = g.push_cover(x, OverrideValue::Value(0)).unwrap();

    g.set(x, 7);
    assert_eq!(g.get(x), 0, "the write went under both covers");
    g.remove_override(x, lower, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 0, "the upper cover is still active");
    g.remove_override(x, upper, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 7, "the property kept its own written value");
}

#[test]
fn restore_binding_mode_brings_binding_back() {
    let g = Graph::new();
    let r = g.root();
    let src = g.property(r, 1).unwrap();
    let x = g.binding(r, 0, move |g| g.get(src) + 1).unwrap();
    let o = g.push_override(x, OverrideValue::Value(50)).unwrap();
    g.set(src, 10);
    g.remove_override(x, o, RestoreMode::Binding).unwrap();
    assert_eq!(g.get(x), 11);
    // RestoreMode::Value over a binding keeps the override's value.
    let o = g.push_override(x, OverrideValue::Value(50)).unwrap();
    g.remove_override(x, o, RestoreMode::Value).unwrap();
    assert_eq!(g.get(x), 50);
    assert!(!g.is_bound(x));
}

#[test]
fn writes_while_overridden() {
    let g = Graph::new();
    let r = g.root();
    let x = g.property(r, 1).unwrap();
    let o = g.push_override(x, OverrideValue::binding(|_| 42)).unwrap();
    assert_eq!(g.get(x), 42);

    // Initial targets the base, invisible until restored.
    g.write(x, 7, WriteKind::Initial).unwrap();
    assert_eq!(g.get(x), 42);

    // Imperative breaks the override's binding, not the base.
    g.set(x, 3);
    assert_eq!(g.get(x), 3);
    assert_eq!(g.override_count(x), 1);

    // Override(id) rewrites that entry.
    g.write(x, 4, WriteKind::Override(o)).unwrap();
    assert_eq!(g.get(x), 4);

    g.remove_override(x, o, RestoreMode::BindingOrValue)
        .unwrap();
    assert_eq!(g.get(x), 7);
}

#[test]
fn frozen_scope_defers_and_coalesces_effects() {
    let g = Graph::new();
    let r = g.root();
    let page = g.create_scope(r).unwrap();
    let inner = g.create_scope(page).unwrap();
    let x = g.property(r, 0).unwrap();
    let doubled = g.binding(inner, 0, move |g| g.get(x) * 2).unwrap();
    let seen = log();
    let s = seen.clone();
    g.watch(inner, x, move |_, v| s.borrow_mut().push(*v))
        .unwrap();

    g.set_frozen(page, true).unwrap();
    assert!(g.is_frozen(inner), "freezing is inherited");
    g.set(x, 1);
    g.set(x, 2);
    g.set(x, 3);
    assert!(seen.borrow().is_empty(), "parked while frozen");
    // Bindings still answer reads while frozen.
    assert_eq!(g.get(doubled), 6);
    // A scope created under a frozen one starts frozen.
    assert!(g.is_frozen(g.create_scope(inner).unwrap()));

    g.set_frozen(page, false).unwrap();
    assert_eq!(*seen.borrow(), vec![3], "one notification, latest value");
    g.set(x, 4);
    assert_eq!(*seen.borrow(), vec![3, 4]);
}

/// A page hidden and frozen in one statement still sees itself hidden: the
/// freeze waits for the effects that change already caused.
#[test]
fn freezing_inside_a_batch_waits_for_the_effects_it_caused() {
    let g = Graph::new();
    let r = g.root();
    let page = g.create_scope(r).unwrap();
    let x = g.property(r, 0).unwrap();
    let seen = log();
    let s = seen.clone();
    g.watch(page, x, move |_, v| s.borrow_mut().push(*v))
        .unwrap();
    g.batch(|| {
        g.set(x, 1);
        g.set_frozen(page, true).unwrap();
        assert!(!g.is_frozen(page), "not before the batch ends");
    });
    assert_eq!(*seen.borrow(), vec![1]);
    assert!(g.is_frozen(page));
    g.set(x, 2);
    assert_eq!(*seen.borrow(), vec![1], "frozen from then on");
    // Thawed before the batch ends: the freeze that waited is dropped.
    g.set_frozen(page, false).unwrap();
    g.batch(|| {
        g.set_frozen(page, true).unwrap();
        g.set_frozen(page, false).unwrap();
    });
    assert!(!g.is_frozen(page));
    assert_eq!(*seen.borrow(), vec![1, 2]);
}

#[test]
fn thawing_a_child_of_a_frozen_parent_stays_frozen() {
    let g: Graph<i64> = Graph::new();
    let parent = g.create_scope(g.root()).unwrap();
    let child = g.create_scope(parent).unwrap();
    g.set_frozen(parent, true).unwrap();
    g.set_frozen(child, false).unwrap();
    assert!(g.is_frozen(child));
    g.set_frozen(child, true).unwrap();
    g.set_frozen(parent, false).unwrap();
    assert!(g.is_frozen(child), "still frozen in its own right");
}

#[test]
fn disposing_a_scope_is_deterministic() {
    let g = Graph::new();
    let r = g.root();
    let order = log();
    let outer = g.create_scope(r).unwrap();
    let child_a = g.create_scope(outer).unwrap();
    let child_b = g.create_scope(outer).unwrap();
    for (s, name) in [(outer, "outer"), (child_a, "a"), (child_b, "b")] {
        let o = order.clone();
        g.on_dispose(s, move || o.borrow_mut().push(name)).unwrap();
    }
    let x = g.property(child_a, 1).unwrap();
    let survivor_src = g.property(r, 1).unwrap();
    let y = g
        .binding(outer, 0, move |g| g.get(x) + g.get(survivor_src))
        .unwrap();
    // An observer outside the scope.
    let watcher = g
        .binding(r, 0, move |g| g.try_get(y).unwrap_or(-1))
        .unwrap();
    assert_eq!(g.get(watcher), 2);

    g.dispose_scope(outer).unwrap();
    assert_eq!(*order.borrow(), vec!["b", "a", "outer"]);
    assert!(!g.is_alive(x) && !g.is_alive(y));
    assert!(!g.scope_alive(child_a));
    assert_eq!(g.get(watcher), -1, "observer re-ran and saw the dead node");
    assert!(g.observers(survivor_src).is_empty());
    assert_eq!(g.dispose_scope(outer), Err(ReactiveError::DeadScope(outer)));
    assert_eq!(g.dispose_scope(r), Err(ReactiveError::RootScope));
    assert_eq!(g.try_get(x), None);
}

#[test]
fn scope_handle_disposes_on_drop() {
    let g: Graph<i64> = Graph::new();
    let n;
    {
        let s = g.scope_handle(g.root()).unwrap();
        n = g.property(s.id(), 1).unwrap();
        assert!(g.is_alive(n));
    }
    assert!(!g.is_alive(n));
    assert!(g.is_empty());
}

#[test]
fn writer_binding_is_promoted_to_effect_root() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let side = g.property(r, 0).unwrap();
    let writer = g
        .binding(r, 0, move |g| {
            let v = g.get(a);
            g.set(side, v * 10);
            v
        })
        .unwrap();
    assert!(!g.is_eager(writer));
    g.get(writer);
    assert!(g.is_eager(writer), "promoted after writing");
    // Now it runs without being read.
    g.set(a, 2);
    assert_eq!(g.state(writer), Some(NodeState::Clean));
    assert_eq!(g.get_untracked(side), 20);
}

#[test]
fn eager_mode_evaluates_without_reads_with_same_results() {
    let build = |eager: bool| {
        let g: Graph<i64> = Graph::with_options(Options {
            eager,
            ..Options::default()
        });
        let r = g.root();
        let a = g.property(r, 1).unwrap();
        let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
        let c = g.binding(r, 0, move |g| g.get(b) * 2).unwrap();
        (g, a, c)
    };
    let (lazy, la, lc) = build(false);
    let (eager, ea, ec) = build(true);
    assert_eq!(lazy.stats().evaluations, 0);
    assert_eq!(eager.stats().evaluations, 2);
    lazy.set(la, 5);
    eager.set(ea, 5);
    assert_eq!(eager.stats().evaluations, 4);
    assert_eq!(lazy.get(lc), eager.get(ec));
}

#[test]
fn set_eager_makes_a_polish_style_root() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let n = counter();
    let b = counted_binding(&g, &n, move |g| g.get(a));
    g.set_eager(b, true).unwrap();
    assert_eq!(n.get(), 1);
    g.set(a, 2);
    assert_eq!(n.get(), 2);
}

#[test]
fn watch_does_not_fire_on_creation_and_can_be_disposed() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
    let seen = log();
    let s = seen.clone();
    let w = g.watch(r, b, move |_, v| s.borrow_mut().push(*v)).unwrap();
    assert!(seen.borrow().is_empty());
    g.set(a, 2);
    assert_eq!(*seen.borrow(), vec![3]);
    g.dispose(w).unwrap();
    g.set(a, 3);
    assert_eq!(*seen.borrow(), vec![3]);
    assert!(g.observers(b).is_empty());
}

#[test]
fn panicking_binding_leaves_graph_usable() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g
        .binding(r, 0, move |g| {
            let v = g.get(a);
            assert!(v != 1, "boom");
            v + 1
        })
        .unwrap();
    assert_eq!(g.get(b), 1);
    g.set(a, 1);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.get(b)));
    assert!(res.is_err());
    assert_eq!(g.state(b), Some(NodeState::Dirty));
    g.set(a, 2);
    assert_eq!(g.get(b), 3, "retried with the closure reinstalled");
}

#[test]
fn effects_can_create_and_dispose_nodes() {
    let g = Graph::new();
    let r = g.root();
    let count = g.property(r, 0).unwrap();
    let made = Rc::new(RefCell::new(Vec::<NodeId>::new()));
    let m = made.clone();
    g.effect(r, move |g| {
        let n = g.get(count);
        for old in m.borrow_mut().drain(..) {
            g.dispose(old).unwrap();
        }
        for i in 0..n {
            let p = g.property(g.root(), i).unwrap();
            m.borrow_mut().push(p);
        }
    })
    .unwrap();
    g.set(count, 3);
    assert_eq!(g.len(), 1 + 1 + 3);
    g.set(count, 1);
    assert_eq!(g.len(), 1 + 1 + 1);
}

#[test]
fn misuse_is_an_error_not_a_panic() {
    let g = Graph::new();
    let r = g.root();
    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(
        g.write(e, 1, WriteKind::Imperative),
        Err(ReactiveError::NotAProperty(e))
    );
    assert_eq!(g.try_get(e), None);
    let p = g.property(r, 1).unwrap();
    g.dispose(p).unwrap();
    assert_eq!(g.dispose(p), Err(ReactiveError::DeadNode(p)));
    assert_eq!(g.notify(p), Err(ReactiveError::DeadNode(p)));
    assert!(g.bind(p, |_| 1).is_err());
}

#[test]
fn heights_follow_dependencies() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a)).unwrap();
    let c = g.binding(r, 0, move |g| g.get(b) + g.get(a)).unwrap();
    g.get(c);
    assert_eq!(
        (g.height(a), g.height(b), g.height(c)),
        (Some(0), Some(1), Some(2))
    );
}

#[test]
fn a_write_in_an_effect_runs_its_own_observers_before_it_returns() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let x = g.property(r, 0).unwrap();
    let seen = Rc::new(Cell::new(0));
    let o = seen.clone();
    g.effect(r, move |g| o.set(g.get(x))).unwrap();
    let after_write = Rc::new(Cell::new(-1));
    let a = after_write.clone();
    let o = seen.clone();
    g.effect(r, move |g| {
        let v = g.get(s);
        if v > 0 {
            g.set(x, v * 10);
            a.set(o.get());
        }
    })
    .unwrap();
    g.set(s, 4);
    assert_eq!(after_write.get(), 40);
}

#[test]
fn effects_waiting_in_one_flush_do_not_run_inside_each_other() {
    // Each effect writes a property of its own, and each write flushes. A
    // few nested flushes run every effect still waiting; past that a write
    // flushes only what it changed, so the nesting stays small however many
    // wait.
    const N: usize = 20_000;
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let depth = Rc::new(Cell::new(0usize));
    let deepest = Rc::new(Cell::new(0usize));
    for _ in 0..N {
        let t = g.property(r, 0).unwrap();
        let (d, m) = (depth.clone(), deepest.clone());
        g.effect(r, move |g| {
            d.set(d.get() + 1);
            m.set(m.get().max(d.get()));
            let v = g.get(s);
            g.batch(|| g.set(t, v));
            d.set(d.get() - 1);
        })
        .unwrap();
    }
    deepest.set(0);
    g.set(s, 1);
    assert!(deepest.get() <= 16, "nested {} deep", deepest.get());
}

/// A run that reads what the last one read keeps its edges as they are;
/// only one that reads something else rebuilds them, and then it follows
/// what it read this time.
#[test]
fn same_reads_keep_their_edges() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let props: Vec<NodeId> = (0..8).map(|i| g.property(r, i).unwrap()).collect();
    let pick = g.property(r, 0).unwrap();
    let extra = g.property(r, 100).unwrap();
    let sum = Rc::new(Cell::new(0));
    let (ps, s) = (props.clone(), sum.clone());
    g.effect(r, move |g| {
        // More sources than fit inline, and one more by choice.
        let mut total: i64 = ps.iter().map(|p| g.get(*p)).sum();
        if g.get(pick) == 1 {
            total += g.get(extra);
        }
        s.set(total);
    })
    .unwrap();
    let total = |g: &Graph<i64>| props.iter().map(|p| g.get(*p)).sum::<i64>();
    let first = g.stats().source_rebuilds;
    let runs = g.stats().effect_runs;
    for i in 0..100 {
        g.set(props[i % 8], i as i64 + 10);
    }
    assert_eq!(g.stats().effect_runs, runs + 100);
    assert_eq!(g.stats().source_rebuilds, first);
    assert_eq!(sum.get(), total(&g));

    // The choice reads one more: rebuilt once, and that one is followed.
    g.set(pick, 1);
    assert_eq!(g.stats().source_rebuilds, first + 1);
    g.set(extra, 7);
    assert_eq!(sum.get(), total(&g) + 7);
    g.set(props[2], 1);
    assert_eq!(g.stats().source_rebuilds, first + 1);
    assert_eq!(sum.get(), total(&g) + 7);

    // And back: it is not followed any more.
    g.set(pick, 0);
    assert_eq!(g.stats().source_rebuilds, first + 2);
    let runs = g.stats().effect_runs;
    g.set(extra, 8);
    assert_eq!(g.stats().effect_runs, runs);
}

#[test]
fn an_effect_that_a_check_finds_dirty_runs_once_though_it_writes() {
    // The effect follows the binding, not the property: the write leaves
    // it to check, and the check finds the binding changed. It then runs
    // there, and its own write flushes what is queued; its place in the
    // queue went with the run.
    let g = Graph::new();
    let a = g.property(g.root(), 0).unwrap();
    let b = g.binding(g.root(), 0, move |g| g.get(a) * 2).unwrap();
    let out = g.property(g.root(), 0).unwrap();
    let n = counter();
    let c = n.clone();
    g.effect(g.root(), move |g| {
        c.set(c.get() + 1);
        let v = g.get(b);
        g.set(out, v);
    })
    .unwrap();
    assert_eq!(n.get(), 1);
    for i in 1..=10 {
        g.set(a, i);
        assert_eq!(n.get(), 1 + i as u32, "write {i}");
    }
    assert_eq!(g.get(out), 20);
}

#[test]
fn an_effect_whose_write_leaves_what_it_read_as_it_was_runs_once() {
    // A text lays itself out (the effect), reads the size that follows its
    // implicit size (the binding), and writes the size of its text, which
    // the binding reads. A binding that gives what it gave is no change:
    // the effect is not run again for it. One that gives more is.
    let g = Graph::new();
    let font = g.property(g.root(), 12).unwrap();
    let content = g.property(g.root(), 0).unwrap();
    let size = g
        .binding(g.root(), 0, move |g| g.get(content).max(24))
        .unwrap();
    let n = counter();
    let c = n.clone();
    g.effect(g.root(), move |g| {
        c.set(c.get() + 1);
        let f = g.get(font);
        let _ = g.get(size);
        g.untrack(|| g.batch(|| g.set(content, f + 2)));
    })
    .unwrap();
    assert_eq!(n.get(), 1);
    g.set(font, 13);
    assert_eq!(n.get(), 2, "the size it read stayed");
    assert_eq!(g.get(size), 24);
    g.set(font, 30);
    assert_eq!(n.get(), 4, "the size it read moved");
    assert_eq!(g.get(size), 32);
}

#[test]
fn an_effect_hears_of_a_change_its_own_write_ran_after_it_first_read() {
    // The first effect reads `src` for the first time in this run, then
    // writes; that write runs the second effect, which changes `src`. The
    // first had no edge to `src` yet, and runs again all the same.
    let g = Graph::new();
    let gate = g.property(g.root(), 0).unwrap();
    let src = g.property(g.root(), 0).unwrap();
    let out = g.property(g.root(), 0).unwrap();
    let seen = Rc::new(Cell::new(-1));
    let s = seen.clone();
    g.effect(g.root(), move |g| {
        if g.get(gate) != 0 {
            s.set(g.get(src));
            g.set(out, 1);
        }
    })
    .unwrap();
    g.effect(g.root(), move |g| {
        if g.get(gate) != 0 {
            g.set(src, 7);
        }
    })
    .unwrap();
    g.set(gate, 1);
    assert_eq!(seen.get(), 7);
}

#[test]
fn only_an_assigned_value_holds_what_a_write_would_write() {
    let g = Graph::new();
    let a = g.property(g.root(), 1).unwrap();
    // Never given a value: a write of it still makes it set.
    assert!(!g.holds_assigned(a, &1));
    g.write(a, 2, WriteKind::Imperative).unwrap();
    assert!(g.holds_assigned(a, &2));
    assert!(!g.holds_assigned(a, &3));
    // A binding is replaced by the write, whatever value it gives.
    let b = g.binding(g.root(), 0, move |g| g.get(a)).unwrap();
    assert_eq!(g.get(b), 2);
    assert!(!g.holds_assigned(b, &2));
    // An animation's step is not the value the property was given.
    g.write(a, 5, WriteKind::Animation).unwrap();
    assert!(!g.holds_assigned(a, &5));
}
