//! What a change made inside an own queue ([`Graph::own_queue`]) reaches
//! of the hooks and told effects waiting below it, for every small graph.
//!
//! A batch writes some sources and `q`, noted. `q`'s hook opens a queue of
//! its own and makes one or two changes in it: a write, a disposal, a
//! reader lost ([`Graph::readers_lost`]). Every hook and told effect that
//! waits below the queue and that such a change reaches is told inside
//! it, before any effect runs there after that change: what the effect
//! reads, the hook keeps up to date. One no change made inside reaches
//! waits for the queue to drop, when every change of the batch waits with
//! it. What a change reaches is worked out here the plain way: what it
//! changed and everything that reads that, however far, whatever state
//! the graph left each node in.
//!
//! What each hook and effect reads is what the sources make of it then,
//! but in a handler ahead ([`Graph::watch_ahead`]): there a binding only
//! changes the queue waits on reached reads as it was, and so does what
//! reads it. One a change made inside the queue reaches reads as it is.
//!
//! Each graph has two sources and three bindings, each reading one or two
//! of what was made before it, every property noted and read by a told
//! effect. Around that: hooks that read their property or not, an effect
//! on every property or none, three shapes of batch, made in a handler
//! ahead or not, three queues (no change waits, the last waits, the
//! batch's all wait), and every change or pair of changes made inside,
//! after as many unrelated changes as the counter of changes went round
//! in, or none.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Instant;

use ljos_reactive::{Graph, NodeId, Reader};

const SOURCES: usize = 2;
const BINDINGS: usize = 3;
const PROPS: usize = SOURCES + BINDINGS;

/// What waits to be told, by the property it is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Entry {
    /// The property's own hook.
    Hook(usize),
    /// The told effect that reads the property.
    Told(usize),
}

/// A change made inside the queue.
#[derive(Clone, Copy, Debug)]
enum Op {
    Write(usize),
    Dispose(usize),
    ReadersLost(usize),
}

/// What the queue `q`'s hook opens leaves waiting.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Since {
    /// No change: `own_queue(changes_so_far())`.
    Now,
    /// The last change counted, as a hook told of it opens one.
    Last,
    /// Every change the batch made.
    Batch,
}

#[derive(Clone, Debug)]
struct Case {
    /// What each binding reads, by index among the properties.
    reads: [Vec<usize>; BINDINGS],
    hooks_read: bool,
    watched: bool,
    /// Sources the batch writes before `q`.
    before: Vec<usize>,
    /// A source it writes after `q`, whose hooks are told before `q`'s.
    after: Option<usize>,
    since: Since,
    /// Whether the batch is made in a handler ahead.
    ahead: bool,
    /// Unrelated changes made inside the queue before `ops`.
    wrap: u16,
    ops: Vec<Op>,
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Told(Entry),
    /// An effect ran.
    Ran,
    Open,
    Op(usize),
    Dropped,
    /// A hook or an effect read property `p`: `got`, when the sources
    /// made it `truth`.
    Read {
        p: usize,
        got: Option<i64>,
        truth: Option<i64>,
    },
}

/// What the sources hold, and what went, as the case runs.
#[derive(Default)]
struct Model {
    sources: [i64; SOURCES],
    gone: [bool; PROPS],
}

impl Model {
    /// What `p` is, from the sources: what went reads as nothing.
    fn truth(&self, reads: &[Vec<usize>; BINDINGS], p: usize) -> Option<i64> {
        if self.gone[p] {
            return None;
        }
        if p < SOURCES {
            return Some(self.sources[p]);
        }
        let sum = reads[p - SOURCES]
            .iter()
            .map(|&x| self.truth(reads, x).unwrap_or(0))
            .sum();
        Some(sum)
    }
}

impl Case {
    /// `from` and every property that reads it, however far.
    fn reach(&self, from: usize) -> BTreeSet<usize> {
        let mut seen = BTreeSet::from([from]);
        let mut stack = vec![from];
        while let Some(x) = stack.pop() {
            for (i, reads) in self.reads.iter().enumerate() {
                if reads.contains(&x) && seen.insert(SOURCES + i) {
                    stack.push(SOURCES + i);
                }
            }
        }
        seen
    }

    /// `p` and every binding it reads, however far.
    fn upstream(&self, p: usize) -> BTreeSet<usize> {
        let mut seen = BTreeSet::from([p]);
        let mut stack = vec![p];
        while let Some(x) = stack.pop() {
            if x >= SOURCES {
                for &y in &self.reads[x - SOURCES] {
                    if seen.insert(y) {
                        stack.push(y);
                    }
                }
            }
        }
        seen.retain(|&x| x >= SOURCES);
        seen
    }

    /// What `op` reaches of what can wait.
    fn reached(&self, op: Op) -> BTreeSet<Entry> {
        let (props, gone) = match op {
            Op::Write(s) => (self.reach(s), None),
            Op::ReadersLost(b) => (self.reach(b), None),
            Op::Dispose(d) => {
                let mut props = self.reach(d);
                props.remove(&d);
                (props, Some(d))
            }
        };
        let mut out: BTreeSet<Entry> = props
            .iter()
            .flat_map(|&p| [Entry::Hook(p), Entry::Told(p)])
            .collect();
        // The told effect of what goes still reads it.
        out.extend(gone.map(Entry::Told));
        out
    }

    /// What waits below the queue when it opens: the hooks of what the
    /// writes before `q` reached, pushed before `q`'s, and every told
    /// effect the batch reached, told after the hooks.
    fn below(&self) -> BTreeSet<Entry> {
        let mut out = BTreeSet::new();
        for &s in &self.before {
            out.extend(self.reach(s).into_iter().map(Entry::Hook));
        }
        for &s in self.before.iter().chain(&self.after) {
            out.extend(self.reach(s).into_iter().map(Entry::Told));
        }
        out
    }

    fn writes(&self) -> u8 {
        (self.before.len() + 1 + usize::from(self.after.is_some())) as u8
    }

    /// The bindings the changes the queue waits on reached, which read as
    /// they were in a handler ahead.
    fn waiting(&self) -> BTreeSet<usize> {
        if !self.ahead || self.since != Since::Batch {
            return BTreeSet::new();
        }
        let mut out = BTreeSet::new();
        for &s in self.before.iter().chain(&self.after) {
            out.extend(self.reach(s).into_iter().filter(|&x| x >= SOURCES));
        }
        out
    }
}

/// Runs `case`, keeping what was told and run, in order.
fn run(case: &Case) -> Vec<Event> {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let log = Rc::new(RefCell::new(Vec::new()));
    let model = Rc::new(RefCell::new(Model::default()));
    let next = Rc::new(Cell::new(0_i64));
    // The value a write of source `s` writes, and the model's of it.
    let fresh = {
        let (next, model) = (Rc::clone(&next), Rc::clone(&model));
        move |s: usize| {
            next.set(next.get() + 1);
            model.borrow_mut().sources[s] = next.get();
            next.get()
        }
    };
    let read = {
        let (log, model, reads) = (Rc::clone(&log), Rc::clone(&model), case.reads.clone());
        move |g: &Graph<i64>, p: usize, n: NodeId| {
            let got = g.try_get(n);
            let truth = model.borrow().truth(&reads, p);
            log.borrow_mut().push(Event::Read { p, got, truth });
        }
    };
    let mut ids: Vec<NodeId> = (0..SOURCES).map(|_| g.property(r, 0).unwrap()).collect();
    let mut readers: Vec<Rc<Cell<Option<Reader>>>> = Vec::new();
    for reads in &case.reads {
        let from: Vec<NodeId> = reads.iter().map(|&x| ids[x]).collect();
        let reader = Rc::new(Cell::new(None));
        let me = Rc::clone(&reader);
        let b = g
            .binding(r, 0, move |g| {
                me.set(g.reader());
                from.iter().map(|&n| g.try_get(n).unwrap_or(0)).sum()
            })
            .unwrap();
        ids.push(b);
        readers.push(reader);
    }
    let q = g.property(r, 0).unwrap();
    let unrelated = g.property(r, 0).unwrap();
    for &n in &ids {
        g.get(n);
    }
    for (p, &n) in ids.iter().enumerate() {
        let (ran, told, reads) = (Rc::clone(&log), Rc::clone(&log), read.clone());
        g.effect_told(
            r,
            move |g| {
                ran.borrow_mut().push(Event::Ran);
                reads(g, p, n);
            },
            move || told.borrow_mut().push(Event::Told(Entry::Told(p))),
        )
        .unwrap();
        if case.watched {
            let (ran, reads) = (Rc::clone(&log), read.clone());
            g.effect(r, move |g| {
                ran.borrow_mut().push(Event::Ran);
                reads(g, p, n);
            })
            .unwrap();
        }
    }
    // Weak: the graph keeps its handler, and a strong handle would keep
    // the graph, and every case run, alive.
    let (weak, l, index) = (Rc::downgrade(&g), Rc::clone(&log), ids.clone());
    let (case_in, fresh_in, model_in) = (case.clone(), fresh.clone(), Rc::clone(&model));
    g.set_note_handler(move |n| {
        let Some(hooks) = weak.upgrade() else {
            return;
        };
        if n != q {
            let p = index.iter().position(|&x| x == n).unwrap();
            l.borrow_mut().push(Event::Told(Entry::Hook(p)));
            if case_in.hooks_read {
                hooks.untrack(|| read(&hooks, p, n));
            }
            return;
        }
        let now = hooks.changes_so_far();
        let own = hooks.own_queue(match case_in.since {
            Since::Now => now,
            Since::Last => now.wrapping_sub(1),
            Since::Batch => now.wrapping_sub(case_in.writes().into()),
        });
        l.borrow_mut().push(Event::Open);
        for v in 1..=case_in.wrap {
            hooks.set(unrelated, i64::from(v));
        }
        for (k, &op) in case_in.ops.iter().enumerate() {
            l.borrow_mut().push(Event::Op(k));
            match op {
                Op::Write(s) => hooks.set(index[s], fresh_in(s)),
                Op::Dispose(d) => {
                    model_in.borrow_mut().gone[d] = true;
                    hooks.dispose(index[d]).unwrap();
                }
                Op::ReadersLost(b) => hooks.readers_lost(readers[b - SOURCES].get()),
            }
        }
        drop(own);
        l.borrow_mut().push(Event::Dropped);
    });
    for &n in ids.iter().chain([&q]) {
        g.set_noted(n).unwrap();
    }
    let batch = {
        let (case, ids) = (case.clone(), ids.clone());
        move |g: &Graph<i64>| {
            g.batch(|| {
                for &s in &case.before {
                    g.set(ids[s], fresh(s));
                }
                g.set(q, 1);
                if let Some(s) = case.after {
                    g.set(ids[s], fresh(s));
                }
            })
        }
    };
    if case.ahead {
        let x = g.property(r, 0).unwrap();
        g.watch_ahead(r, x, move |g, _| batch(g)).unwrap();
        log.borrow_mut().clear();
        g.set(x, 1);
    } else {
        log.borrow_mut().clear();
        batch(&g);
    }

    log.take()
}

/// Whether `events` keep to what the reference says of `case`.
fn check(case: &Case, events: &[Event]) -> Result<(), String> {
    let below = case.below();
    let at = |want: fn(&Event) -> bool| events.iter().position(want);
    let open = at(|e| matches!(e, Event::Open)).ok_or("the queue never opened")?;
    let dropped = at(|e| matches!(e, Event::Dropped)).ok_or("the queue never dropped")?;
    for e in &events[..open] {
        if let Event::Told(x) = e
            && below.contains(x)
        {
            return Err(format!("reference: {x:?} was told before the queue opened"));
        }
    }
    // What a change made inside has reached so far, and the bindings the
    // changes the queue waits on reached that none made inside has: what
    // reads one of those may read it as it was.
    let mut inside = BTreeSet::new();
    let waiting = case.waiting();
    for (at, e) in events.iter().enumerate() {
        if let Event::Op(k) = *e {
            inside.extend(
                case.reached(case.ops[k])
                    .into_iter()
                    .filter_map(|x| match x {
                        Entry::Hook(p) => Some(p),
                        Entry::Told(_) => None,
                    }),
            );
        }
        if let Event::Read { p, got, truth } = *e
            && got.is_some()
            && got != truth
        {
            let as_it_was = at > open
                && case
                    .upstream(p)
                    .iter()
                    .any(|y| waiting.contains(y) && !inside.contains(y));
            if !as_it_was {
                return Err(format!(
                    "property {p} read {got:?} when its sources made it {truth:?}{}",
                    if inside.contains(&p) {
                        ", and a change made inside the queue reached it"
                    } else {
                        ""
                    }
                ));
            }
        }
    }
    let mut reached = BTreeSet::new();
    let mut gone = BTreeSet::new();
    let mut told = BTreeSet::new();
    for e in &events[open + 1..dropped] {
        match *e {
            Event::Op(k) => {
                let op = case.ops[k];
                reached.extend(case.reached(op).intersection(&below).copied());
                if let Op::Dispose(d) = op {
                    gone.insert(Entry::Hook(d));
                }
            }
            Event::Told(x) => {
                told.insert(x);
            }
            Event::Ran => {
                if let Some(x) = reached.difference(&told).find(|x| !gone.contains(*x)) {
                    return Err(format!(
                        "an effect ran inside the queue while {x:?}, which a change made \
                         inside reached, waited below it"
                    ));
                }
            }
            Event::Open | Event::Dropped | Event::Read { .. } => {}
        }
    }
    if let Some(x) = reached.difference(&told).find(|x| !gone.contains(*x)) {
        return Err(format!(
            "{x:?}, which a change made inside reached, waited for the queue"
        ));
    }
    if case.since == Since::Batch
        && let Some(x) = told.intersection(&below).find(|x| !reached.contains(*x))
    {
        return Err(format!(
            "{x:?}, which no change made inside reached, was told inside"
        ));
    }
    let after: BTreeSet<Entry> = events[open..]
        .iter()
        .filter_map(|e| match e {
            Event::Told(x) => Some(*x),
            _ => None,
        })
        .collect();
    if let Some(x) = below.difference(&after).find(|x| !gone.contains(*x)) {
        return Err(format!("{x:?} was never told"));
    }
    Ok(())
}

/// Every non-empty set of at most two of `0..n`.
fn reads_of(n: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = (0..n).map(|a| vec![a]).collect();
    for a in 0..n {
        for b in a + 1..n {
            out.push(vec![a, b]);
        }
    }
    out
}

/// Every case of the graphs and batches, in a handler ahead or not, for
/// each of `wraps` and of what `ops` gives of the changes made inside.
fn cases(wraps: &[u16], pairs: bool, all_hooks: bool) -> Vec<Case> {
    let mut graphs = Vec::new();
    for a in reads_of(SOURCES) {
        for b in reads_of(SOURCES + 1) {
            for c in reads_of(SOURCES + 2) {
                graphs.push([a.clone(), b.clone(), c]);
            }
        }
    }
    let changes: Vec<Op> = (0..SOURCES)
        .map(Op::Write)
        .chain((SOURCES..PROPS).map(Op::ReadersLost))
        .collect();
    let mut ops: Vec<Vec<Op>> = changes
        .iter()
        .chain(&(0..PROPS).map(Op::Dispose).collect::<Vec<_>>())
        .map(|&op| vec![op])
        .collect();
    if pairs {
        for &a in &changes {
            for &b in &changes {
                ops.push(vec![a, b]);
            }
        }
    }
    let flags: &[bool] = if all_hooks { &[false, true] } else { &[true] };
    // The other orders and the other source are the same graphs relabelled.
    let batches = [vec![], vec![0], vec![0, 1]];
    let afters = [None, Some(0), Some(1)];
    let mut out = Vec::new();
    for reads in &graphs {
        for &hooks_read in flags {
            for &watched in flags {
                for before in &batches {
                    for &after in &afters {
                        for since in [Since::Now, Since::Last, Since::Batch] {
                            for ahead in [false, true] {
                                for &wrap in wraps {
                                    for ops in &ops {
                                        out.push(Case {
                                            reads: reads.clone(),
                                            hooks_read,
                                            watched,
                                            before: before.clone(),
                                            after,
                                            since,
                                            ahead,
                                            wrap,
                                            ops: ops.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn what_a_change_inside_an_own_queue_reaches_is_told_inside_it_for_every_small_graph() {
    sweep(cases(&[0], true, true));
}

/// The changes made inside come after as many unrelated ones as bring a
/// counter of a byte round to each change the queue waits on, and to the
/// one the handler ahead is told of: a change made inside is a change of
/// its own however many came before it.
#[test]
fn what_a_change_inside_an_own_queue_reaches_is_told_inside_it_however_many_came_before() {
    sweep(cases(&(249..=256).collect::<Vec<_>>(), false, false));
}

fn sweep(cases: Vec<Case>) {
    let mut total = 0;
    let mut failed = 0;
    let mut by_op = [0_usize; 3];
    let mut shown = String::new();
    let began = Instant::now();
    for case in cases {
        total += 1;
        let events = run(&case);
        if let Err(why) = check(&case, &events) {
            failed += 1;
            let kind = match case.ops.last() {
                Some(Op::Write(_)) => 0,
                Some(Op::Dispose(_)) => 1,
                _ => 2,
            };
            by_op[kind] += 1;
            if failed <= 6 {
                let _ = writeln!(shown, "{why}\n  in {case:?}\n  events {events:?}");
            }
        }
    }
    eprintln!("{total} cases in {:?}", began.elapsed());
    assert_eq!(
        failed, 0,
        "{failed} of {total} cases failed (last change a write: {}, a disposal: {}, a reader \
         lost: {}); the first:\n{shown}",
        by_op[0], by_op[1], by_op[2]
    );
}

/// The cases review found, one by one, among the ones above.
#[test]
fn the_cases_review_found_are_among_them() {
    let case = |reads: [Vec<usize>; BINDINGS], since, ahead, wrap, op| Case {
        reads,
        hooks_read: true,
        watched: true,
        before: vec![0],
        after: None,
        since,
        ahead,
        wrap,
        ops: vec![op],
    };
    let found = [
        (
            "a disposal inside the queue",
            case(
                [vec![0, 1], vec![2], vec![3]],
                Since::Last,
                false,
                0,
                Op::Dispose(1),
            ),
        ),
        (
            "a write inside the queue that makes a binding waiting to be checked dirty",
            case(
                [vec![0], vec![2, 1], vec![3]],
                Since::Now,
                false,
                0,
                Op::Write(1),
            ),
        ),
        (
            "a write inside the queue after 255 unrelated ones",
            case(
                [vec![0], vec![2], vec![3]],
                Since::Last,
                false,
                255,
                Op::Write(0),
            ),
        ),
        (
            "a write inside the queue, in a handler ahead",
            case(
                [vec![0], vec![0], vec![0]],
                Since::Batch,
                true,
                0,
                Op::Write(0),
            ),
        ),
    ];
    let failed: Vec<String> = found
        .iter()
        .filter_map(|(what, case)| {
            check(case, &run(case))
                .err()
                .map(|why| format!("{what}: {why}"))
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
