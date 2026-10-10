//! Invariants of the O(1) edge and scope bookkeeping. Random sequences of
//! graph operations must never leave a back-index pointing at the wrong
//! entry, whatever order nodes and scopes are bound, rebound, dropped and
//! disposed in.

use ljos_reactive::{Graph, NodeId, OverrideId, OverrideValue, RestoreMode, ScopeId};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Op {
    NewScope(usize),
    NewProp(usize, i64),
    /// Reads `reads`; when `cond`, only the first plus one of the rest,
    /// chosen by the first one's value, so dependencies change over time.
    NewBinding(usize, Vec<usize>, bool),
    NewWatch(usize, usize),
    NewEffect(usize, Vec<usize>),
    Rebind(usize, Vec<usize>, bool),
    Set(usize, i64),
    Refresh(usize),
    DisposeNode(usize),
    DisposeScope(usize),
    Read(usize),
    Push(usize, Vec<usize>),
    Pop(usize),
    Freeze(usize, bool),
}

fn op() -> impl Strategy<Value = Op> {
    let ix = 0usize..64;
    let reads = || prop::collection::vec(0usize..64, 1..5);
    prop_oneof![
        2 => ix.clone().prop_map(Op::NewScope),
        4 => (ix.clone(), 0i64..4).prop_map(|(s, v)| Op::NewProp(s, v)),
        6 => (ix.clone(), reads(), any::<bool>()).prop_map(|(s, r, c)| Op::NewBinding(s, r, c)),
        2 => (ix.clone(), ix.clone()).prop_map(|(s, t)| Op::NewWatch(s, t)),
        1 => (ix.clone(), reads()).prop_map(|(s, r)| Op::NewEffect(s, r)),
        3 => (ix.clone(), reads(), any::<bool>()).prop_map(|(n, r, c)| Op::Rebind(n, r, c)),
        4 => (ix.clone(), 0i64..4).prop_map(|(n, v)| Op::Set(n, v)),
        2 => ix.clone().prop_map(Op::Refresh),
        3 => ix.clone().prop_map(Op::DisposeNode),
        2 => ix.clone().prop_map(Op::DisposeScope),
        3 => ix.clone().prop_map(Op::Read),
        1 => (ix.clone(), reads()).prop_map(|(n, r)| Op::Push(n, r)),
        1 => ix.clone().prop_map(Op::Pop),
        1 => (ix, any::<bool>()).prop_map(|(s, f)| Op::Freeze(s, f)),
    ]
}

fn pick<T: Copy>(v: &[T], i: usize) -> T {
    v[i % v.len()]
}

fn sum_of(g: &Graph<i64>, reads: &[NodeId], cond: bool) -> i64 {
    let mut total = 0;
    for (k, n) in reads.iter().enumerate() {
        let v = g.try_get(*n).unwrap_or(0);
        total += v;
        // The branch taken depends on the first read.
        if cond && k == 0 && v % 2 == 0 {
            break;
        }
    }
    total % 5
}

/// The two edge lists mirror each other as seen through the public API too.
fn mirrored(g: &Graph<i64>, nodes: &[NodeId]) {
    for &n in nodes {
        for s in g.sources(n) {
            assert!(
                g.observers(s).contains(&n),
                "{n:?} reads {s:?} but is not in its observers"
            );
        }
        for o in g.observers(n) {
            assert!(
                g.sources(o).contains(&n),
                "{o:?} observes {n:?} but does not list it"
            );
        }
    }
}

fn run(ops: &[Op], eager: bool) {
    let g: Graph<i64> = Graph::with_options(ljos_reactive::Options {
        eager,
        ..Default::default()
    });
    let mut scopes: Vec<ScopeId> = vec![g.root()];
    let mut nodes: Vec<NodeId> = vec![g.property(g.root(), 0).unwrap()];
    let mut overrides: Vec<(NodeId, OverrideId)> = Vec::new();

    for (step, op) in ops.iter().enumerate() {
        let gather = |nodes: &[NodeId], r: &[usize]| -> Vec<NodeId> {
            r.iter().map(|i| pick(nodes, *i)).collect()
        };
        match op {
            Op::NewScope(p) => {
                if let Ok(s) = g.create_scope(pick(&scopes, *p)) {
                    scopes.push(s);
                }
            }
            Op::NewProp(s, v) => {
                if let Ok(n) = g.property(pick(&scopes, *s), *v) {
                    nodes.push(n);
                }
            }
            Op::NewBinding(s, r, cond) => {
                let reads = gather(&nodes, r);
                let cond = *cond;
                if let Ok(n) = g.binding(pick(&scopes, *s), 0, move |g| sum_of(g, &reads, cond)) {
                    nodes.push(n);
                }
            }
            Op::NewWatch(s, t) => {
                if let Ok(n) = g.watch(pick(&scopes, *s), pick(&nodes, *t), |_, _| {}) {
                    nodes.push(n);
                }
            }
            Op::NewEffect(s, r) => {
                let reads = gather(&nodes, r);
                if let Ok(n) = g.effect(pick(&scopes, *s), move |g| {
                    sum_of(g, &reads, false);
                }) {
                    nodes.push(n);
                }
            }
            Op::Rebind(n, r, cond) => {
                let reads = gather(&nodes, r);
                let cond = *cond;
                let _ = g.bind(pick(&nodes, *n), move |g| sum_of(g, &reads, cond));
            }
            Op::Set(n, v) => {
                let _ = g.write(pick(&nodes, *n), *v, ljos_reactive::WriteKind::Imperative);
            }
            Op::Refresh(n) => {
                let _ = g.refresh(pick(&nodes, *n));
            }
            Op::DisposeNode(n) => {
                let _ = g.dispose(pick(&nodes, *n));
            }
            Op::DisposeScope(s) => {
                let _ = g.dispose_scope(pick(&scopes, *s));
            }
            Op::Read(n) => {
                let _ = g.try_get(pick(&nodes, *n));
            }
            Op::Push(n, r) => {
                let reads = gather(&nodes, r);
                let n = pick(&nodes, *n);
                if let Ok(id) =
                    g.push_override(n, OverrideValue::binding(move |g| sum_of(g, &reads, true)))
                {
                    overrides.push((n, id));
                }
            }
            Op::Pop(i) => {
                if !overrides.is_empty() {
                    let (n, id) = overrides.remove(*i % overrides.len());
                    let _ = g.remove_override(n, id, RestoreMode::BindingOrValue);
                }
            }
            Op::Freeze(s, f) => {
                let _ = g.set_frozen(pick(&scopes, *s), *f);
            }
        }
        if let Err(e) = g.check_invariants() {
            panic!("step {step} ({op:?}): {e}");
        }
        mirrored(&g, &nodes);
        g.take_diagnostics();
    }

    // Tear everything down: no entry may dangle, and the graph ends empty.
    for s in scopes.iter().skip(1) {
        let _ = g.dispose_scope(*s);
        g.check_invariants().unwrap();
    }
    for n in &nodes {
        let _ = g.dispose(*n);
        g.check_invariants().unwrap();
    }
    assert!(g.is_empty(), "{} nodes leaked", g.len());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn back_indices_stay_consistent(ops in prop::collection::vec(op(), 1..80)) {
        run(&ops, false);
    }

    #[test]
    fn back_indices_stay_consistent_eager(ops in prop::collection::vec(op(), 1..80)) {
        run(&ops, true);
    }
}

/// Scopes dispose newest first even after siblings were removed from the
/// middle of the child list.
#[test]
fn child_scopes_keep_creation_order_after_removals() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let g: Graph<i64> = Graph::new();
    let root = g.root();
    let parent = g.create_scope(root).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    let kids: Vec<ScopeId> = (0..6)
        .map(|i| {
            let k = g.create_scope(parent).unwrap();
            let log = log.clone();
            g.on_dispose(k, move || log.borrow_mut().push(i)).unwrap();
            k
        })
        .collect();
    g.dispose_scope(kids[2]).unwrap();
    g.dispose_scope(kids[5]).unwrap();
    g.dispose_scope(kids[0]).unwrap();
    log.borrow_mut().clear();
    g.dispose_scope(parent).unwrap();
    assert_eq!(*log.borrow(), vec![4, 3, 1]);
    g.check_invariants().unwrap();
}
