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
//! after as many changes counted as would bring a narrower count round, or
//! none.
//!
//! The cases are independent, so they share the cores, each core running
//! its cases one after another on a graph of its own, each case in a
//! scope that goes when it ends; the reference works in sets of bits, as
//! the graphs are this small. That keeps the two million cases inside a
//! debug build's time.

use std::cell::{Cell, RefCell};
use std::fmt::{self, Write as _};
use std::rc::Rc;
use std::sync::LazyLock;
use std::thread;
use std::time::Instant;

use ljos_reactive::{Change, Graph, NodeId, Reader, ScopeId};

const SOURCES: usize = 2;
const BINDINGS: usize = 3;
const PROPS: usize = SOURCES + BINDINGS;

/// A set of properties, by index: bit `p` holds property `p`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Props(u8);

impl Props {
    fn of(ps: &[usize]) -> Self {
        Props(ps.iter().fold(0, |m, &p| m | 1 << p))
    }

    fn has(self, p: usize) -> bool {
        self.0 >> p & 1 == 1
    }

    fn with(self, p: usize) -> Self {
        Props(self.0 | 1 << p)
    }

    fn without(self, p: usize) -> Self {
        Props(self.0 & !(1 << p))
    }

    fn union(self, other: Self) -> Self {
        Props(self.0 | other.0)
    }

    fn meets(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    fn bindings(self) -> Self {
        Props(self.0 & !((1 << SOURCES) - 1))
    }

    fn iter(self) -> impl Iterator<Item = usize> {
        (0..PROPS).filter(move |&p| self.has(p))
    }
}

impl fmt::Debug for Props {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// What waits to be told, by the property it is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// The property's own hook.
    Hook(usize),
    /// The told effect that reads the property.
    Told(usize),
}

/// A set of entries: bit `p` holds `Hook(p)`, bit `PROPS + p` `Told(p)`,
/// so the lowest is the first a sorted set of them would give.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Entries(u16);

impl Entries {
    fn bit(e: Entry) -> u16 {
        match e {
            Entry::Hook(p) => 1 << p,
            Entry::Told(p) => 1 << (PROPS + p),
        }
    }

    fn hooks(ps: Props) -> Self {
        Entries(u16::from(ps.0))
    }

    fn told(ps: Props) -> Self {
        Entries(u16::from(ps.0) << PROPS)
    }

    fn has(self, e: Entry) -> bool {
        self.0 & Self::bit(e) != 0
    }

    fn insert(&mut self, e: Entry) {
        self.0 |= Self::bit(e);
    }

    fn union(self, other: Self) -> Self {
        Entries(self.0 | other.0)
    }

    fn and(self, other: Self) -> Self {
        Entries(self.0 & other.0)
    }

    fn minus(self, other: Self) -> Self {
        Entries(self.0 & !other.0)
    }

    fn of_hooks(self) -> Props {
        Props((self.0 & ((1 << PROPS) - 1)) as u8)
    }

    fn first(self) -> Option<Entry> {
        let at = self.0.trailing_zeros() as usize;
        match at {
            _ if self.0 == 0 => None,
            p if p < PROPS => Some(Entry::Hook(p)),
            p => Some(Entry::Told(p - PROPS)),
        }
    }
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

#[derive(Clone, Copy, Debug)]
struct Case {
    /// What each binding reads, among the properties.
    reads: [Props; BINDINGS],
    hooks_read: bool,
    watched: bool,
    /// Sources the batch writes before `q`.
    before: &'static [usize],
    /// A source it writes after `q`, whose hooks are told before `q`'s.
    after: Option<usize>,
    since: Since,
    /// Whether the batch is made in a handler ahead.
    ahead: bool,
    /// Unrelated changes written inside the queue before `ops`.
    written: u16,
    /// Changes counted inside the queue after those, before `ops`,
    /// without being made ([`Graph::count_changes`]).
    counted: Change,
    ops: &'static [Op],
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
    fn truth(&self, reads: &[Props; BINDINGS], p: usize) -> Option<i64> {
        if self.gone[p] {
            return None;
        }
        if p < SOURCES {
            return Some(self.sources[p]);
        }
        let sum = reads[p - SOURCES]
            .iter()
            .map(|x| self.truth(reads, x).unwrap_or(0))
            .sum();
        Some(sum)
    }
}

impl Case {
    /// `from` and every property that reads it, however far. A binding
    /// reads only what was made before it, so one pass in that order
    /// sees all.
    fn reach(&self, from: usize) -> Props {
        let mut seen = Props::of(&[from]);
        for (i, &reads) in self.reads.iter().enumerate() {
            if reads.meets(seen) {
                seen = seen.with(SOURCES + i);
            }
        }
        seen
    }

    /// The bindings among `p` and everything it reads, however far.
    fn upstream(&self, p: usize) -> Props {
        let mut seen = Props::of(&[p]);
        for i in (0..BINDINGS).rev() {
            if seen.has(SOURCES + i) {
                seen = seen.union(self.reads[i]);
            }
        }
        seen.bindings()
    }

    /// What `op` reaches of what can wait.
    fn reached(&self, op: Op) -> Entries {
        let (props, gone) = match op {
            Op::Write(s) | Op::ReadersLost(s) => (self.reach(s), None),
            Op::Dispose(d) => (self.reach(d).without(d), Some(d)),
        };
        let mut out = Entries::hooks(props).union(Entries::told(props));
        // The told effect of what goes still reads it.
        if let Some(d) = gone {
            out.insert(Entry::Told(d));
        }
        out
    }

    /// What waits below the queue when it opens: the hooks of what the
    /// writes before `q` reached, pushed before `q`'s, and every told
    /// effect the batch reached, told after the hooks.
    fn below(&self) -> Entries {
        let mut out = Entries::default();
        for &s in self.before {
            out = out.union(Entries::hooks(self.reach(s)));
        }
        for &s in self.before.iter().chain(&self.after) {
            out = out.union(Entries::told(self.reach(s)));
        }
        out
    }

    fn writes(&self) -> u8 {
        (self.before.len() + 1 + usize::from(self.after.is_some())) as u8
    }

    /// The bindings the changes the queue waits on reached, which read as
    /// they were in a handler ahead.
    fn waiting(&self) -> Props {
        if !self.ahead || self.since != Since::Batch {
            return Props::default();
        }
        let mut out = Props::default();
        for &s in self.before.iter().chain(&self.after) {
            out = out.union(self.reach(s).bindings());
        }
        out
    }
}

/// Runs `case` on a graph of its own, keeping what was told and run, in
/// order.
fn run(case: Case) -> Vec<Event> {
    Bench::new().run(case)
}

/// A graph cases run on one after another, each in a scope of its own
/// that goes when it ends. The room a graph's first node makes, which
/// was most of what a case cost on a graph of its own, is made once; and
/// each case starts as far along the count of changes as the ones before
/// it took it.
struct Bench {
    g: Rc<Graph<i64>>,
    /// The nodes live between cases: one, in the root, which keeps the
    /// room.
    idle: usize,
}

impl Bench {
    fn new() -> Self {
        let g = Rc::new(Graph::new());
        g.property(g.root(), 0).unwrap();
        let idle = g.len();
        Bench { g, idle }
    }

    /// Runs `case`, keeping what was told and run, in order. What it made
    /// goes after, all of it.
    fn run(&self, case: Case) -> Vec<Event> {
        let g = &self.g;
        let r = g.create_scope(g.root()).unwrap();
        let events = case_in(g, r, case);
        g.set_note_handler(|_| {});
        g.dispose_scope(r).unwrap();
        assert_eq!(g.len(), self.idle, "nodes outlived {case:?}");
        events
    }
}

/// Runs `case` in scope `r` of `g`.
fn case_in(g: &Rc<Graph<i64>>, r: ScopeId, case: Case) -> Vec<Event> {
    let log = Rc::new(RefCell::new(Vec::with_capacity(64)));
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
        let (log, model) = (Rc::clone(&log), Rc::clone(&model));
        move |g: &Graph<i64>, p: usize, n: NodeId| {
            let got = g.try_get(n);
            let truth = model.borrow().truth(&case.reads, p);
            log.borrow_mut().push(Event::Read { p, got, truth });
        }
    };
    let mut ids: Vec<NodeId> = (0..SOURCES).map(|_| g.property(r, 0).unwrap()).collect();
    let mut readers: Vec<Rc<Cell<Option<Reader>>>> = Vec::new();
    for reads in case.reads {
        let from: Vec<NodeId> = reads.iter().map(|x| ids[x]).collect();
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
    // the graph alive.
    let (weak, l, index) = (Rc::downgrade(g), Rc::clone(&log), ids.clone());
    let (fresh_in, model_in) = (fresh.clone(), Rc::clone(&model));
    g.set_note_handler(move |n| {
        let Some(hooks) = weak.upgrade() else {
            return;
        };
        if n != q {
            let p = index.iter().position(|&x| x == n).unwrap();
            l.borrow_mut().push(Event::Told(Entry::Hook(p)));
            if case.hooks_read {
                hooks.untrack(|| read(&hooks, p, n));
            }
            return;
        }
        let now = hooks.changes_so_far();
        let own = hooks.own_queue(match case.since {
            Since::Now => now,
            Since::Last => now.wrapping_sub(1),
            Since::Batch => now.wrapping_sub(case.writes().into()),
        });
        l.borrow_mut().push(Event::Open);
        for v in 1..=case.written {
            hooks.set(unrelated, i64::from(v));
        }
        hooks.count_changes(case.counted);
        for (k, &op) in case.ops.iter().enumerate() {
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
        let ids = ids.clone();
        move |g: &Graph<i64>| {
            g.batch(|| {
                for &s in case.before {
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
        batch(g);
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
        if let Event::Told(x) = *e
            && below.has(x)
        {
            return Err(format!("reference: {x:?} was told before the queue opened"));
        }
    }
    // What a change made inside has reached so far, and the bindings the
    // changes the queue waits on reached that none made inside has: what
    // reads one of those may read it as it was.
    let mut inside = Props::default();
    let waiting = case.waiting();
    for (at, e) in events.iter().enumerate() {
        if let Event::Op(k) = *e {
            inside = inside.union(case.reached(case.ops[k]).of_hooks());
        }
        if let Event::Read { p, got, truth } = *e
            && got.is_some()
            && got != truth
        {
            let as_it_was = at > open
                && case
                    .upstream(p)
                    .iter()
                    .any(|y| waiting.has(y) && !inside.has(y));
            if !as_it_was {
                return Err(format!(
                    "property {p} read {got:?} when its sources made it {truth:?}{}",
                    if inside.has(p) {
                        ", and a change made inside the queue reached it"
                    } else {
                        ""
                    }
                ));
            }
        }
    }
    let mut reached = Entries::default();
    let mut gone = Entries::default();
    let mut told = Entries::default();
    for e in &events[open + 1..dropped] {
        match *e {
            Event::Op(k) => {
                let op = case.ops[k];
                reached = reached.union(case.reached(op).and(below));
                if let Op::Dispose(d) = op {
                    gone.insert(Entry::Hook(d));
                }
            }
            Event::Told(x) => told.insert(x),
            Event::Ran => {
                if let Some(x) = reached.minus(told).minus(gone).first() {
                    return Err(format!(
                        "an effect ran inside the queue while {x:?}, which a change made \
                         inside reached, waited below it"
                    ));
                }
            }
            Event::Open | Event::Dropped | Event::Read { .. } => {}
        }
    }
    if let Some(x) = reached.minus(told).minus(gone).first() {
        return Err(format!(
            "{x:?}, which a change made inside reached, waited for the queue"
        ));
    }
    if case.since == Since::Batch
        && let Some(x) = told.and(below).minus(reached).first()
    {
        return Err(format!(
            "{x:?}, which no change made inside reached, was told inside"
        ));
    }
    let mut after = Entries::default();
    for e in &events[open..] {
        if let Event::Told(x) = *e {
            after.insert(x);
        }
    }
    if let Some(x) = below.minus(after).minus(gone).first() {
        return Err(format!("{x:?} was never told"));
    }
    Ok(())
}

/// Every non-empty set of at most two of `0..n`.
fn reads_of(n: usize) -> Vec<Props> {
    let mut out: Vec<Props> = (0..n).map(|a| Props::of(&[a])).collect();
    for a in 0..n {
        for b in a + 1..n {
            out.push(Props::of(&[a, b]));
        }
    }
    out
}

/// Every small graph: what each binding reads.
fn graphs() -> Vec<[Props; BINDINGS]> {
    let mut out = Vec::new();
    for a in reads_of(SOURCES) {
        for b in reads_of(SOURCES + 1) {
            for c in reads_of(SOURCES + 2) {
                out.push([a, b, c]);
            }
        }
    }
    out
}

/// Every change made inside alone, then every pair of the ones that
/// leave what they change there.
static OPS: LazyLock<Vec<Vec<Op>>> = LazyLock::new(|| {
    let changes: Vec<Op> = (0..SOURCES)
        .map(Op::Write)
        .chain((SOURCES..PROPS).map(Op::ReadersLost))
        .collect();
    let mut ops: Vec<Vec<Op>> = changes
        .iter()
        .copied()
        .chain((0..PROPS).map(Op::Dispose))
        .map(|op| vec![op])
        .collect();
    for &a in &changes {
        for &b in &changes {
            ops.push(vec![a, b]);
        }
    }
    ops
});

/// What runs around each graph.
struct Space {
    /// Changes counted before the ones made inside, one run for each.
    counted: &'static [Change],
    /// Pairs of changes made inside, as well as each alone.
    pairs: bool,
    /// Hooks that do not read and no effect on every property, as well as
    /// both.
    all_hooks: bool,
}

/// The other orders and the other source are the same graphs relabelled.
const BATCHES: [&[usize]; 3] = [&[], &[0], &[0, 1]];
const AFTERS: [Option<usize>; 3] = [None, Some(0), Some(1)];

impl Space {
    /// Every case of `reads`, into `out`.
    fn cases(&self, reads: [Props; BINDINGS], out: &mut Vec<Case>) {
        let singles = 2 * PROPS;
        let ops = if self.pairs {
            &OPS[..]
        } else {
            &OPS[..singles]
        };
        let flags: &[bool] = if self.all_hooks {
            &[false, true]
        } else {
            &[true]
        };
        out.clear();
        for &hooks_read in flags {
            for &watched in flags {
                for before in BATCHES {
                    for after in AFTERS {
                        for since in [Since::Now, Since::Last, Since::Batch] {
                            for ahead in [false, true] {
                                for &counted in self.counted {
                                    for ops in ops {
                                        out.push(Case {
                                            reads,
                                            hooks_read,
                                            watched,
                                            before,
                                            after,
                                            since,
                                            ahead,
                                            written: 0,
                                            counted,
                                            ops,
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
}

#[test]
fn what_a_change_inside_an_own_queue_reaches_is_told_inside_it_for_every_small_graph() {
    sweep(&Space {
        counted: &[0],
        pairs: true,
        all_hooks: true,
    });
}

/// As many changes counted before the changes made inside as bring a
/// count of 8, 16 or 32 bits round to each change the batch made, up to
/// four, and to the one before them, which in a handler ahead is the write
/// that sets it off: the changes a count narrower than the graph's would
/// take a change made inside for. A count of a byte went wrong after 252
/// to 255 unrelated writes; these, and 251, are the same offsets from
/// 2^32. A change
/// made inside is a change of its own however many came before it.
/// Counting them is as writing them (`changes_counted_are_as_unrelated_writes`),
/// only fast.
#[test]
fn what_a_change_inside_an_own_queue_reaches_is_told_inside_it_however_many_came_before() {
    const ROUND: Change = 1 << 32;
    sweep(&Space {
        counted: &[ROUND - 5, ROUND - 4, ROUND - 3, ROUND - 2, ROUND - 1],
        pairs: false,
        all_hooks: false,
    });
}

/// What each part of the cases came to.
#[derive(Default)]
struct Tally {
    total: usize,
    failed: usize,
    /// By the last change made inside: a write, a disposal, a reader lost.
    by_op: [usize; 3],
    /// The first failures, described.
    shown: Vec<String>,
}

const SHOWN: usize = 6;

fn sweep(space: &Space) {
    let began = Instant::now();
    let graphs = graphs();
    let workers = thread::available_parallelism().map_or(4, usize::from);
    let parts: Vec<Tally> = thread::scope(|scope| {
        let handles: Vec<_> = graphs
            .chunks(graphs.len().div_ceil(workers).max(1))
            .map(|chunk| scope.spawn(move || tally(space, chunk)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut all = Tally::default();
    for part in parts {
        all.total += part.total;
        all.failed += part.failed;
        for (a, b) in all.by_op.iter_mut().zip(part.by_op) {
            *a += b;
        }
        all.shown.extend(part.shown);
    }
    all.shown.truncate(SHOWN);
    let Tally {
        total,
        failed,
        by_op,
        shown,
    } = all;
    eprintln!("{total} cases in {:?}", began.elapsed());
    assert_eq!(
        failed,
        0,
        "{failed} of {total} cases failed (last change a write: {}, a disposal: {}, a reader \
         lost: {}); the first:\n{}",
        by_op[0],
        by_op[1],
        by_op[2],
        shown.concat()
    );
}

/// Runs and checks every case of `graphs`.
fn tally(space: &Space, graphs: &[[Props; BINDINGS]]) -> Tally {
    let mut out = Tally::default();
    let mut cases = Vec::new();
    let bench = Bench::new();
    for &reads in graphs {
        space.cases(reads, &mut cases);
        for case in &cases {
            out.total += 1;
            let events = bench.run(*case);
            if let Err(why) = check(case, &events) {
                out.failed += 1;
                let kind = match case.ops.last() {
                    Some(Op::Write(_)) => 0,
                    Some(Op::Dispose(_)) => 1,
                    _ => 2,
                };
                out.by_op[kind] += 1;
                if out.shown.len() < SHOWN {
                    let mut shown = String::new();
                    let _ = writeln!(shown, "{why}\n  in {case:?}\n  events {events:?}");
                    out.shown.push(shown);
                }
            }
        }
    }
    out
}

/// Changes counted ([`Graph::count_changes`]) are, to all a case sees, as
/// many unrelated changing writes: every case of the graph review found
/// the count of a byte wrong in runs the same either way, after a few and
/// after as many as went round.
#[test]
fn changes_counted_are_as_unrelated_writes() {
    let space = Space {
        counted: &[0],
        pairs: false,
        all_hooks: false,
    };
    let mut cases = Vec::new();
    space.cases(
        [Props::of(&[0]), Props::of(&[2]), Props::of(&[3])],
        &mut cases,
    );
    for case in cases {
        for n in [1, 255] {
            let written = run(Case { written: n, ..case });
            let counted = run(Case {
                counted: n.into(),
                ..case
            });
            assert_eq!(written, counted, "{n} written or counted in {case:?}");
        }
    }
}

/// The cases review found, one by one, among the ones above.
#[test]
fn the_cases_review_found_are_among_them() {
    let case = |reads: [&[usize]; BINDINGS], since, ahead, written, ops: &'static [Op]| Case {
        reads: reads.map(Props::of),
        hooks_read: true,
        watched: true,
        before: &[0],
        after: None,
        since,
        ahead,
        written,
        counted: 0,
        ops,
    };
    let found = [
        (
            "a disposal inside the queue",
            case(
                [&[0, 1], &[2], &[3]],
                Since::Last,
                false,
                0,
                &[Op::Dispose(1)],
            ),
        ),
        (
            "a write inside the queue that makes a binding waiting to be checked dirty",
            case([&[0], &[2, 1], &[3]], Since::Now, false, 0, &[Op::Write(1)]),
        ),
        (
            "a write inside the queue after 255 unrelated ones",
            case([&[0], &[2], &[3]], Since::Last, false, 255, &[Op::Write(0)]),
        ),
        (
            "a write inside the queue, in a handler ahead",
            case([&[0], &[0], &[0]], Since::Batch, true, 0, &[Op::Write(0)]),
        ),
    ];
    let failed: Vec<String> = found
        .iter()
        .filter_map(|(what, case)| {
            check(case, &run(*case))
                .err()
                .map(|why| format!("{what}: {why}"))
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
