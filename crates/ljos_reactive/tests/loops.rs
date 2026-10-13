//! Binding loops: what counts as one, which node is reported and what the
//! values settle to. The expected numbers are what Qt 6.11.2 (`qml6`, run as
//! a black box) leaves in the same properties.

use std::cell::Cell;
use std::rc::Rc;

use ljos_reactive::{Diagnostic, Graph, NodeId, Options, WriteKind};

fn graph(eager: bool) -> Graph<i64> {
    Graph::with_options(Options {
        eager,
        ..Options::default()
    })
}

/// `n` properties starting at 0, created in order.
fn props<const N: usize>(g: &Graph<i64>) -> [NodeId; N] {
    std::array::from_fn(|_| g.property(g.root(), 0).unwrap())
}

fn loops(g: &Graph<i64>) -> Vec<NodeId> {
    g.take_diagnostics()
        .into_iter()
        .map(|d| match d {
            Diagnostic::BindingLoop { node, .. } => node,
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

/// Reads every node a few times: a broken loop must not move again.
fn settled<const N: usize>(g: &Graph<i64>, nodes: [NodeId; N]) -> [i64; N] {
    let first = nodes.map(|n| g.get(n));
    for _ in 0..3 {
        assert_eq!(nodes.map(|n| g.get(n)), first, "values keep moving");
    }
    first
}

#[test]
fn an_acyclic_chain_is_no_loop_in_any_order() {
    // p = 1, q = p + 1, r = q + 1, bound in every order.
    let orders: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for eager in [false, true] {
        for order in orders {
            let g = graph(eager);
            let [p, q, r] = props(&g);
            g.batch(|| {
                for which in order {
                    match which {
                        0 => g.bind(p, |_| 1).unwrap(),
                        1 => g.bind(q, move |g| g.get(p) + 1).unwrap(),
                        _ => g.bind(r, move |g| g.get(q) + 1).unwrap(),
                    }
                }
            });
            assert_eq!(settled(&g, [p, q, r]), [1, 2, 3], "{eager} {order:?}");
            assert_eq!(loops(&g), [], "{eager} {order:?}");
        }
    }
}

#[test]
fn a_flush_nested_in_a_binding_is_no_loop() {
    // While `q` computes, a write flushes the queue and runs `r`, which
    // reads `q`. `q` does not depend on `r`: a stale read, not a loop.
    let g = graph(true);
    let [side, p, q, r] = props(&g);
    g.batch(|| {
        g.bind(q, move |g| {
            g.write(side, 7, WriteKind::Initial).unwrap();
            g.get(p) + 1
        })
        .unwrap();
        g.bind(r, move |g| g.get(q) + 1).unwrap();
    });
    assert_eq!(loops(&g), []);
    assert_eq!(settled(&g, [q, r]), [1, 2]);
    g.set(p, 10);
    assert_eq!(loops(&g), []);
    assert_eq!(settled(&g, [q, r]), [11, 12]);
}

#[test]
fn an_effect_that_reads_a_computing_binding_is_no_loop() {
    let g = graph(false);
    let [side, p, q] = props(&g);
    let seen = Rc::new(Cell::new(-1));
    let s = seen.clone();
    g.bind(q, move |g| {
        g.set(side, g.get_untracked(side) + 1);
        g.get(p) + 1
    })
    .unwrap();
    g.batch(|| {
        g.effect(g.root(), move |g| s.set(g.get(q))).unwrap();
    });
    assert_eq!(loops(&g), []);
    assert_eq!(seen.get(), 1);
    g.set(p, 4);
    assert_eq!(loops(&g), []);
    assert_eq!(seen.get(), 5);
}

#[test]
fn a_two_cycle_reports_the_first_evaluated_and_settles() {
    // Qt: `p: q + 1; q: p + 1` warns for p and leaves p 2, q 3.
    for eager in [false, true] {
        let g = graph(eager);
        let [p, q] = props(&g);
        g.batch(|| {
            g.bind(p, move |g| g.get(q) + 1).unwrap();
            g.bind(q, move |g| g.get(p) + 1).unwrap();
        });
        assert_eq!(settled(&g, [p, q]), [2, 3], "eager {eager}");
        assert_eq!(loops(&g), [p], "eager {eager}");
    }
}

#[test]
fn the_other_declaration_order_reports_the_other_member() {
    // Qt: `q: p + 1; p: q + 1` warns for q and leaves p 3, q 2.
    let g = graph(true);
    let [q, p] = props(&g);
    g.batch(|| {
        g.bind(q, move |g| g.get(p) + 1).unwrap();
        g.bind(p, move |g| g.get(q) + 1).unwrap();
    });
    assert_eq!(settled(&g, [p, q]), [3, 2]);
    assert_eq!(loops(&g), [q]);
}

#[test]
fn a_three_cycle_matches_qt() {
    // Qt: `a: b + 1; b: c + 1; c: a + 1` warns for a; a 3, b 5, c 4.
    for eager in [false, true] {
        let g = graph(eager);
        let [a, b, c] = props(&g);
        g.batch(|| {
            g.bind(a, move |g| g.get(b) + 1).unwrap();
            g.bind(b, move |g| g.get(c) + 1).unwrap();
            g.bind(c, move |g| g.get(a) + 1).unwrap();
        });
        assert_eq!(settled(&g, [a, b, c]), [3, 5, 4], "eager {eager}");
        let diags = g.take_diagnostics();
        assert_eq!(
            diags,
            [Diagnostic::BindingLoop {
                node: a,
                cycle: vec![a, b, c]
            }],
            "eager {eager}"
        );
    }
    // Qt: `a: c + 1; b: a + 1; c: b + 1` warns for a; a 3, b 4, c 5.
    let g = graph(true);
    let [a, b, c] = props(&g);
    g.batch(|| {
        g.bind(a, move |g| g.get(c) + 1).unwrap();
        g.bind(b, move |g| g.get(a) + 1).unwrap();
        g.bind(c, move |g| g.get(b) + 1).unwrap();
    });
    assert_eq!(settled(&g, [a, b, c]), [3, 4, 5]);
    assert_eq!(loops(&g), [a]);
}

// The products are the QML under test, written as it is there: each binding
// reads the other and still always gives 0.
#[allow(clippy::erasing_op)]
#[test]
fn a_cycle_whose_values_do_not_change_is_not_reported() {
    // Qt: `p: q * 0; q: p * 0; s: s * 1` logs nothing.
    for eager in [false, true] {
        let g = graph(eager);
        let [p, q, s] = props(&g);
        g.batch(|| {
            g.bind(p, move |g| g.get(q) * 0).unwrap();
            g.bind(q, move |g| g.get(p) * 0).unwrap();
            g.bind(s, move |g| g.get(s)).unwrap();
        });
        assert_eq!(settled(&g, [p, q, s]), [0, 0, 0]);
        assert_eq!(loops(&g), [], "eager {eager}");
    }
}

#[test]
fn a_self_loop_is_reported_once_and_runs_once() {
    // Qt: `s: s + 1` warns for s and leaves 1.
    for eager in [false, true] {
        let g = graph(eager);
        let [s] = props(&g);
        g.bind(s, move |g| g.get(s) + 1).unwrap();
        assert_eq!(settled(&g, [s]), [1]);
        let diags = g.take_diagnostics();
        assert_eq!(
            diags,
            [Diagnostic::BindingLoop {
                node: s,
                cycle: vec![s]
            }]
        );
    }
}

#[test]
fn every_new_trigger_reports_again_and_settles_again() {
    // Qt, `p: q + x; q: p + 1`: p 1, q 2 at first; x = 5 gives p 7, q 8;
    // x = 7 gives p 15, q 16. Each step warns once, for p.
    for eager in [false, true] {
        let g = graph(eager);
        let [x, p, q] = props(&g);
        g.batch(|| {
            g.bind(p, move |g| g.get(q) + g.get(x)).unwrap();
            g.bind(q, move |g| g.get(p) + 1).unwrap();
        });
        assert_eq!(settled(&g, [p, q]), [1, 2], "eager {eager}");
        assert_eq!(loops(&g), [p]);
        g.set(x, 5);
        assert_eq!(settled(&g, [p, q]), [7, 8], "eager {eager}");
        assert_eq!(loops(&g), [p]);
        g.set(x, 7);
        assert_eq!(settled(&g, [p, q]), [15, 16], "eager {eager}");
        assert_eq!(loops(&g), [p]);
        g.check_invariants().unwrap();
    }
}

#[test]
fn a_trigger_on_the_second_member_reports_that_member() {
    // `p: q + 1; q: p + y`. Changing y re-runs q first, so the change
    // comes back to q.
    for eager in [false, true] {
        let g = graph(eager);
        let [y, p, q] = props(&g);
        g.set(y, 1);
        g.batch(|| {
            g.bind(p, move |g| g.get(q) + 1).unwrap();
            g.bind(q, move |g| g.get(p) + g.get(y)).unwrap();
        });
        assert_eq!(settled(&g, [p, q]), [2, 3], "eager {eager}");
        assert_eq!(loops(&g), [p]);
        g.set(y, 10);
        assert_eq!(settled(&g, [q, p]), [12, 13], "eager {eager}");
        assert_eq!(loops(&g), [q]);
    }
}

#[test]
fn closing_a_cycle_at_run_time_reports_the_new_binding() {
    // Qt: `p = Qt.binding(() => q + 1)` gives p 1; then
    // `q = Qt.binding(() => p + 1)` warns for q and leaves p 3, q 2.
    for eager in [false, true] {
        let g = graph(eager);
        let [p, q] = props(&g);
        g.bind(p, move |g| g.get(q) + 1).unwrap();
        assert_eq!(g.get(p), 1);
        g.bind(q, move |g| g.get(p) + 1).unwrap();
        assert_eq!(settled(&g, [q, p]), [2, 3], "eager {eager}");
        assert_eq!(loops(&g), [q]);
    }
}

#[test]
fn a_watcher_sees_each_settled_value_once() {
    let g = graph(false);
    let [x, p, q] = props(&g);
    g.batch(|| {
        g.bind(p, move |g| g.get(q) + g.get(x)).unwrap();
        g.bind(q, move |g| g.get(p) + 1).unwrap();
    });
    assert_eq!(settled(&g, [p, q]), [1, 2]);
    assert_eq!(loops(&g), [p]);
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    g.watch(g.root(), q, move |_, v| s.borrow_mut().push(*v))
        .unwrap();
    g.set(x, 5);
    assert_eq!(*seen.borrow(), [8]);
    assert_eq!(loops(&g), [p]);
    assert_eq!(g.get(p), 7);
}

#[test]
fn a_panicking_member_leaves_the_graph_usable() {
    let g = graph(false);
    let [x, p, q] = props(&g);
    g.bind(p, move |g| g.get(q) + g.get(x)).unwrap();
    let armed = Rc::new(Cell::new(false));
    let fuse = armed.clone();
    g.bind(q, move |g| {
        let v = g.get(p);
        assert!(!fuse.replace(false), "boom");
        v + 1
    })
    .unwrap();
    assert_eq!(settled(&g, [p, q]), [1, 2]);
    g.take_diagnostics();
    // q panics while p's cycle settles.
    armed.set(true);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.set(x, 5);
        g.get(p)
    }));
    assert!(r.is_err());
    g.check_invariants().unwrap();
    // q gave no value, so p's change never came back round: no loop yet.
    assert_eq!(loops(&g), []);
    // p is not left settling: q's retry is a new trigger, which comes
    // back to q and moves p once more.
    assert_eq!(g.get(q), 8);
    assert_eq!(settled(&g, [p, q]), [13, 8]);
    assert_eq!(loops(&g), [q]);
    g.set(x, 20);
    assert_eq!(settled(&g, [p, q]), [28, 29]);
}
