//! Differential test: the graph against a naive model that re-evaluates
//! every node after every operation.
//!
//! Random DAGs mix static (`Sum`) and dynamic (`Cond`) dependencies, with
//! values mod 3 so equality cut-offs happen often. After each operation the
//! model predicts, from the dependencies captured on each node's previous
//! run, exactly which nodes must re-run. We check:
//!
//! - values (on reads, from effects, and at the end) match the model;
//! - eager mode re-evaluates exactly the predicted bindings, and no binding
//!   runs twice per operation in either mode;
//! - effects run exactly when predicted, see consistent values, and run in
//!   (height, creation) order.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use ljos_reactive::{Graph, NodeId, Options};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Def {
    Sum(Vec<usize>),
    Cond(usize, usize, usize),
}

impl Def {
    fn run(&self, read: &mut dyn FnMut(usize) -> i64) -> i64 {
        match self {
            Def::Sum(xs) => xs.iter().map(|x| read(*x)).sum::<i64>() % 3,
            Def::Cond(c, a, b) => {
                if read(*c) % 2 == 0 {
                    read(*a)
                } else {
                    (read(*b) + 1) % 3
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Spec {
    sources: usize,
    computed: Vec<Def>,
    effects: Vec<Def>,
}

#[derive(Clone, Debug)]
enum Op {
    Set(usize, i64),
    Batch(Vec<(usize, i64)>),
    Read(usize),
}

fn def(bound: usize) -> impl Strategy<Value = Def> {
    prop_oneof![
        prop::collection::vec(0..bound, 1..4).prop_map(Def::Sum),
        (0..bound, 0..bound, 0..bound).prop_map(|(c, a, b)| Def::Cond(c, a, b)),
    ]
}

fn spec() -> impl Strategy<Value = Spec> {
    (1usize..5, 0usize..14, 0usize..5).prop_flat_map(|(sources, n_computed, n_effects)| {
        let computed: Vec<_> = (0..n_computed).map(|i| def(sources + i)).collect();
        let effects = prop::collection::vec(def(sources + n_computed), n_effects);
        (Just(sources), computed, effects).prop_map(|(sources, computed, effects)| Spec {
            sources,
            computed,
            effects,
        })
    })
}

fn ops(sources: usize, total: usize) -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        3 => (0..sources, 0i64..3).prop_map(|(s, v)| Op::Set(s, v)),
        1 => prop::collection::vec((0..sources, 0i64..3), 1..4).prop_map(Op::Batch),
        1 => (0..total).prop_map(Op::Read),
    ];
    prop::collection::vec(op, 0..30)
}

// ------------------------------------------------------------------ model

struct Model {
    spec: Spec,
    vals: Vec<i64>,
    /// Dependencies captured by each computed node's last run.
    deps: Vec<BTreeSet<usize>>,
    effect_deps: Vec<BTreeSet<usize>>,
}

struct Expect {
    evals: BTreeSet<usize>,
    effects: BTreeSet<usize>,
}

impl Model {
    fn new(spec: Spec) -> Self {
        let mut m = Model {
            vals: vec![0; spec.sources + spec.computed.len()],
            deps: vec![BTreeSet::new(); spec.computed.len()],
            effect_deps: vec![BTreeSet::new(); spec.effects.len()],
            spec,
        };
        m.recompute(&BTreeSet::new());
        m
    }

    /// Re-evaluates everything; `touched` are sources written with a
    /// different value (even if a later write in the batch undid it).
    fn recompute(&mut self, touched: &BTreeSet<usize>) -> Expect {
        let s = self.spec.sources;
        let mut changed = touched.clone();
        let mut evals = BTreeSet::new();
        for (i, d) in self.spec.computed.iter().enumerate() {
            let node = s + i;
            if !self.deps[i].is_disjoint(&changed) {
                evals.insert(node);
            }
            let mut deps = BTreeSet::new();
            let vals = &self.vals;
            let v = d.run(&mut |x| {
                deps.insert(x);
                vals[x]
            });
            if v != self.vals[node] {
                changed.insert(node);
            }
            self.vals[node] = v;
            self.deps[i] = deps;
        }
        let mut effects = BTreeSet::new();
        for (e, d) in self.spec.effects.iter().enumerate() {
            if !self.effect_deps[e].is_disjoint(&changed) {
                effects.insert(e);
            }
            let mut deps = BTreeSet::new();
            let vals = &self.vals;
            d.run(&mut |x| {
                deps.insert(x);
                vals[x]
            });
            self.effect_deps[e] = deps;
        }
        Expect { evals, effects }
    }

    fn apply(&mut self, writes: &[(usize, i64)]) -> Expect {
        let mut touched = BTreeSet::new();
        for &(src, v) in writes {
            if self.vals[src] != v {
                touched.insert(src);
            }
            self.vals[src] = v;
        }
        self.recompute(&touched)
    }
}

// ------------------------------------------------------------------ graph

#[derive(Debug)]
enum Event {
    Eval(usize),
    /// (effect index, its height when it started, values it read)
    Effect(usize, u32, Vec<(usize, i64)>),
}

struct Harness {
    g: Rc<Graph<i64>>,
    ids: Rc<RefCell<Vec<NodeId>>>,
    log: Rc<RefCell<Vec<Event>>>,
}

impl Harness {
    fn new(spec: &Spec, eager: bool) -> Self {
        let g = Rc::new(Graph::with_options(Options {
            eager,
            ..Options::default()
        }));
        let ids = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::new(RefCell::new(Vec::new()));
        let root = g.root();
        for _ in 0..spec.sources {
            let id = g.property(root, 0).unwrap();
            ids.borrow_mut().push(id);
        }
        for (i, d) in spec.computed.iter().enumerate() {
            let node = spec.sources + i;
            let (d, ids2, log2) = (d.clone(), ids.clone(), log.clone());
            let id = g
                .binding(root, 0, move |g| {
                    log2.borrow_mut().push(Event::Eval(node));
                    d.run(&mut |x| {
                        let id = ids2.borrow()[x];
                        g.get(id)
                    })
                })
                .unwrap();
            ids.borrow_mut().push(id);
        }
        for (e, d) in spec.effects.iter().enumerate() {
            let (d, ids2, log2) = (d.clone(), ids.clone(), log.clone());
            let me: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
            let me2 = me.clone();
            let id = g
                .effect(root, move |g| {
                    let h = me2.get().and_then(|n| g.height(n)).unwrap_or(0);
                    let mut seen = Vec::new();
                    d.run(&mut |x| {
                        let id = ids2.borrow()[x];
                        let v = g.get(id);
                        seen.push((x, v));
                        v
                    });
                    log2.borrow_mut().push(Event::Effect(e, h, seen));
                })
                .unwrap();
            me.set(Some(id));
        }
        Harness { g, ids, log }
    }

    fn id(&self, x: usize) -> NodeId {
        self.ids.borrow()[x]
    }

    fn take(&self) -> Vec<Event> {
        std::mem::take(&mut *self.log.borrow_mut())
    }
}

fn check_op(h: &Harness, model: &Model, expect: &Expect, eager: bool) -> Result<(), TestCaseError> {
    let events = h.take();
    let mut evals = Vec::new();
    let mut effects = Vec::new();
    for ev in events {
        match ev {
            Event::Eval(n) => evals.push(n),
            Event::Effect(e, height, seen) => {
                for (x, v) in &seen {
                    prop_assert_eq!(*v, model.vals[*x], "effect {} read stale node {}", e, x);
                }
                effects.push((height, e));
            }
        }
    }
    let eval_set: BTreeSet<_> = evals.iter().copied().collect();
    prop_assert_eq!(
        eval_set.len(),
        evals.len(),
        "a binding ran twice: {:?}",
        evals
    );
    // Lazy mode may run a binding dirtied by an earlier operation once
    // something finally reads it, so only eager mode is exact.
    if eager {
        prop_assert_eq!(&eval_set, &expect.evals);
    }
    let effect_set: BTreeSet<_> = effects.iter().map(|(_, e)| *e).collect();
    prop_assert_eq!(effect_set.len(), effects.len(), "an effect ran twice");
    prop_assert_eq!(&effect_set, &expect.effects);
    let mut sorted = effects.clone();
    sorted.sort();
    prop_assert_eq!(sorted, effects, "effects not in height order");
    Ok(())
}

fn run(spec: Spec, ops: Vec<Op>, eager: bool) -> Result<(), TestCaseError> {
    let mut model = Model::new(spec.clone());
    let h = Harness::new(&spec, eager);
    h.take();
    let g = h.g.clone();
    for op in ops {
        let expect = match &op {
            Op::Set(s, v) => {
                g.set(h.id(*s), *v);
                model.apply(&[(*s, *v)])
            }
            Op::Batch(ws) => {
                g.batch(|| {
                    for (s, v) in ws {
                        g.set(h.id(*s), *v);
                    }
                });
                model.apply(ws)
            }
            Op::Read(x) => {
                let v = g.get(h.id(*x));
                prop_assert_eq!(v, model.vals[*x], "read of node {}", x);
                // Reading is not a change: no effect runs, and in eager mode
                // nothing is dirty to evaluate.
                Expect {
                    evals: BTreeSet::new(),
                    effects: BTreeSet::new(),
                }
            }
        };
        check_op(&h, &model, &expect, eager)?;
    }
    for x in 0..model.vals.len() {
        prop_assert_eq!(
            g.get_untracked(h.id(x)),
            model.vals[x],
            "final value of {}",
            x
        );
    }
    prop_assert!(g.take_diagnostics().is_empty());
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn eager_graph_matches_full_reevaluation(
        (spec, ops) in spec().prop_flat_map(|s| {
            let total = s.sources + s.computed.len();
            let src = s.sources;
            (Just(s), ops(src, total))
        })
    ) {
        run(spec, ops, true)?;
    }

    #[test]
    fn lazy_graph_matches_full_reevaluation(
        (spec, ops) in spec().prop_flat_map(|s| {
            let total = s.sources + s.computed.len();
            let src = s.sources;
            (Just(s), ops(src, total))
        })
    ) {
        run(spec, ops, false)?;
    }
}

// ---------------------------------------------------------------- freezing

/// ARCHITECTURE.md §8.3: with and without freezing, observers end with the
/// same values, and a frozen observer is told about the latest value.
fn run_freezing(spec: Spec, ops: Vec<Op>, freeze_at: Vec<bool>) -> Result<(), TestCaseError> {
    let total = spec.sources + spec.computed.len();
    let outcome = |freeze: bool| {
        let h = Harness::new(&spec, false);
        let g = h.g.clone();
        let page = g.create_scope(g.root()).unwrap();
        let last: Rc<RefCell<Vec<Option<i64>>>> = Rc::new(RefCell::new(vec![None; total]));
        for x in 0..total {
            let last = last.clone();
            g.watch(page, h.id(x), move |_, v| last.borrow_mut()[x] = Some(*v))
                .unwrap();
        }
        for (op, f) in ops
            .iter()
            .zip(freeze_at.iter().chain(std::iter::repeat(&false)))
        {
            if freeze {
                g.set_frozen(page, *f).unwrap();
            }
            match op {
                Op::Set(s, v) => g.set(h.id(*s), *v),
                Op::Batch(ws) => g.batch(|| ws.iter().for_each(|(s, v)| g.set(h.id(*s), *v))),
                Op::Read(x) => {
                    g.get(h.id(*x));
                }
            }
        }
        g.set_frozen(page, false).unwrap();
        let finals: Vec<i64> = (0..total).map(|x| g.get_untracked(h.id(x))).collect();
        let last = last.borrow().clone();
        (finals, last)
    };
    let (plain_vals, plain_last) = outcome(false);
    let (frozen_vals, frozen_last) = outcome(true);
    prop_assert_eq!(&plain_vals, &frozen_vals);
    for x in 0..total {
        if let Some(v) = frozen_last[x] {
            prop_assert_eq!(
                v,
                frozen_vals[x],
                "watcher {} saw a non-final value last",
                x
            );
        }
        // A source watcher hears about every change, so presence matches.
        if x < spec.sources {
            prop_assert_eq!(plain_last[x].is_some(), frozen_last[x].is_some());
        }
        if let Some(v) = plain_last[x] {
            prop_assert_eq!(v, plain_vals[x]);
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn freezing_preserves_final_values(
        (spec, ops, freeze_at) in spec().prop_flat_map(|s| {
            let total = s.sources + s.computed.len();
            let src = s.sources;
            (Just(s), ops(src, total), prop::collection::vec(any::<bool>(), 0..30))
        })
    ) {
        run_freezing(spec, ops, freeze_at)?;
    }
}
