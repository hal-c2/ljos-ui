//! The supervisor tree: strategies, restart kinds, intensity on a virtual
//! clock, escalation and linked supervisors (ARCHITECTURE.md 9.4).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use ljos_runtime::{
    ChildId, ChildKind, ChildSpec, Clock, ExitReason, Fault, Intensity, Outcome, Restart,
    StartError, Strategy, SupId, SupervisorError, SupervisorTree, VirtualClock,
};

type Log = Vec<String>;

struct Leaf {
    name: &'static str,
    /// How many of the next starts fail.
    fail: Cell<u32>,
}

fn leaf(name: &'static str) -> Rc<Leaf> {
    Rc::new(Leaf {
        name,
        fail: Cell::new(0),
    })
}

impl ChildSpec<Log> for Leaf {
    fn kind(&self) -> ChildKind {
        ChildKind::Other("test")
    }

    fn start(&self, log: &mut Log) -> Result<(), Fault> {
        if self.fail.get() > 0 {
            self.fail.set(self.fail.get() - 1);
            log.push(format!("fail {}", self.name));
            return Err(Fault::Startup(format!("{} cannot start", self.name)));
        }
        log.push(format!("start {}", self.name));
        Ok(())
    }

    fn stop(&self, log: &mut Log, reason: &ExitReason) {
        match reason {
            ExitReason::Fault(f) => log.push(format!("stop {} ({f})", self.name)),
            _ => log.push(format!("stop {}", self.name)),
        }
    }
}

fn boom() -> Fault {
    Fault::Panic("boom".into())
}

fn lenient() -> Intensity {
    Intensity::new(10, Duration::from_secs(60))
}

/// A tree with children a, b, c under the root.
fn abc(strategy: Strategy, restart: Restart) -> (SupervisorTree<Log>, [ChildId; 3]) {
    let tree = SupervisorTree::new("root", strategy, lenient());
    let ids = ["a", "b", "c"].map(|n| {
        tree.add_child(tree.root(), n, restart, leaf(n))
            .expect("the root exists")
    });
    (tree, ids)
}

#[test]
fn one_for_one_restarts_only_the_child_that_exited() {
    let (tree, [_, b, _]) = abc(Strategy::OneForOne, Restart::Permanent);
    let clock = VirtualClock::new();
    let mut log = Log::new();
    let out = tree
        .child_exited(b, ExitReason::Fault(boom()), clock.now(), &mut log)
        .unwrap();
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![b]
        }
    );
    // It exited by itself, so it is not stopped.
    assert_eq!(log, ["start b"]);
    assert!(tree.children(tree.root()).iter().all(|c| c.running));
}

#[test]
fn one_for_all_stops_the_others_last_first_and_starts_all_in_order() {
    let (tree, [a, b, c]) = abc(Strategy::OneForAll, Restart::Permanent);
    let mut log = Log::new();
    let out = tree
        .child_exited(
            b,
            ExitReason::Fault(boom()),
            VirtualClock::new().now(),
            &mut log,
        )
        .unwrap();
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![a, b, c]
        }
    );
    assert_eq!(log, ["stop c", "stop a", "start a", "start b", "start c"]);
}

#[test]
fn rest_for_one_restarts_the_child_and_those_started_after_it() {
    let (tree, [_, b, c]) = abc(Strategy::RestForOne, Restart::Permanent);
    let mut log = Log::new();
    let out = tree
        .child_exited(
            b,
            ExitReason::Fault(boom()),
            VirtualClock::new().now(),
            &mut log,
        )
        .unwrap();
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![b, c]
        }
    );
    assert_eq!(log, ["stop c", "start b", "start c"]);
}

#[test]
fn a_failed_child_is_stopped_with_its_fault_before_it_restarts() {
    let (tree, [a, _, _]) = abc(Strategy::OneForOne, Restart::Permanent);
    let mut log = Log::new();
    tree.child_failed(a, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert_eq!(log, ["stop a (panic: boom)", "start a"]);
}

#[test]
fn a_transient_child_restarts_after_a_fault_only() {
    let (tree, [a, b, _]) = abc(Strategy::OneForOne, Restart::Transient);
    let now = VirtualClock::new().now();
    let mut log = Log::new();
    let out = tree
        .child_exited(a, ExitReason::Normal, now, &mut log)
        .unwrap();
    assert_eq!(out, Outcome::Stopped { child: a });
    assert!(log.is_empty());
    // It stays in the tree, stopped.
    let kids = tree.children(tree.root());
    assert!(!kids.iter().find(|c| c.id == a).unwrap().running);
    let out = tree
        .child_exited(b, ExitReason::Fault(boom()), now, &mut log)
        .unwrap();
    assert!(matches!(out, Outcome::Restarted { .. }));
    assert_eq!(log, ["start b"]);
}

#[test]
fn a_temporary_child_is_never_restarted_and_is_forgotten() {
    let (tree, [a, _, _]) = abc(Strategy::OneForOne, Restart::Temporary);
    let mut log = Log::new();
    let out = tree
        .child_failed(a, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert_eq!(out, Outcome::Stopped { child: a });
    assert_eq!(log, ["stop a (panic: boom)"]);
    assert_eq!(tree.owner(a), None);
    assert_eq!(tree.children(tree.root()).len(), 2);
    assert_eq!(
        tree.child_exited(a, ExitReason::Normal, VirtualClock::new().now(), &mut log),
        Err(SupervisorError::UnknownChild(a))
    );
}

#[test]
fn more_restarts_than_the_intensity_allows_make_the_root_give_up() {
    let tree: SupervisorTree<Log> = SupervisorTree::new(
        "root",
        Strategy::OneForOne,
        Intensity::new(2, Duration::from_secs(1)),
    );
    let a = tree
        .add_child(tree.root(), "a", Restart::Permanent, leaf("a"))
        .unwrap();
    let b = tree
        .add_child(tree.root(), "b", Restart::Permanent, leaf("b"))
        .unwrap();
    let clock = VirtualClock::new();
    let mut log = Log::new();
    for _ in 0..2 {
        clock.advance_ms(100);
        let out = tree.child_failed(a, boom(), clock.now(), &mut log).unwrap();
        assert!(matches!(out, Outcome::Restarted { .. }));
    }
    assert_eq!(tree.recent_restarts(tree.root()), 2);
    log.clear();
    clock.advance_ms(100);
    let out = tree.child_failed(a, boom(), clock.now(), &mut log).unwrap();
    assert_eq!(
        out,
        Outcome::GaveUp {
            sup: tree.root(),
            reason: ExitReason::Fault(boom())
        }
    );
    // The one that failed with its fault, then what is left, last first.
    assert_eq!(log, ["stop a (panic: boom)", "stop b"]);
    assert!(tree.is_down(tree.root()));
    assert!(tree.children(tree.root()).iter().all(|c| !c.running));
    let _ = b;
}

#[test]
fn restarts_older_than_the_window_do_not_count() {
    let tree: SupervisorTree<Log> = SupervisorTree::new(
        "root",
        Strategy::OneForOne,
        Intensity::new(2, Duration::from_secs(1)),
    );
    let a = tree
        .add_child(tree.root(), "a", Restart::Permanent, leaf("a"))
        .unwrap();
    let clock = VirtualClock::new();
    let mut log = Log::new();
    for _ in 0..10 {
        // Two restarts a second, for as long as one likes.
        clock.advance_ms(500);
        let out = tree.child_failed(a, boom(), clock.now(), &mut log).unwrap();
        assert!(matches!(out, Outcome::Restarted { .. }), "{out:?}");
    }
    assert_eq!(tree.recent_restarts(tree.root()), 2);
}

#[test]
fn a_supervisor_that_gives_up_is_its_parents_child_to_restart() {
    let tree: SupervisorTree<Log> = SupervisorTree::new("root", Strategy::OneForOne, lenient());
    let sub = tree
        .add_supervisor(
            tree.root(),
            "sub",
            Restart::Permanent,
            Strategy::OneForOne,
            Intensity::new(1, Duration::from_secs(1)),
        )
        .unwrap();
    let other = tree
        .add_child(tree.root(), "other", Restart::Permanent, leaf("other"))
        .unwrap();
    let a = tree
        .add_child(sub, "a", Restart::Permanent, leaf("a"))
        .unwrap();
    let b = tree
        .add_child(sub, "b", Restart::Permanent, leaf("b"))
        .unwrap();
    assert_eq!(tree.parent(sub), Some(tree.root()));
    let clock = VirtualClock::new();
    let mut log = Log::new();
    tree.child_failed(a, boom(), clock.now(), &mut log).unwrap();
    log.clear();
    let out = tree.child_failed(a, boom(), clock.now(), &mut log).unwrap();
    // `sub` gave up: its children are stopped, then the root restarts it
    // whole. `other` is not touched (one for one).
    assert_eq!(
        log,
        ["stop a (panic: boom)", "stop b", "start a", "start b"]
    );
    let sub_child = tree.children(tree.root())[0].id;
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![sub_child]
        }
    );
    assert!(!tree.is_down(sub));
    // A restarted supervisor starts with a clean record.
    assert_eq!(tree.recent_restarts(sub), 0);
    assert_eq!(tree.recent_restarts(tree.root()), 1);
    assert!(tree.children(sub).iter().all(|c| c.running));
    let _ = (b, other);
}

#[test]
fn a_temporary_supervisor_that_gives_up_is_forgotten() {
    let tree: SupervisorTree<Log> = SupervisorTree::new("root", Strategy::OneForOne, lenient());
    let sub = tree
        .add_supervisor(
            tree.root(),
            "sub",
            Restart::Temporary,
            Strategy::OneForOne,
            Intensity::new(0, Duration::from_secs(1)),
        )
        .unwrap();
    let a = tree
        .add_child(sub, "a", Restart::Permanent, leaf("a"))
        .unwrap();
    let sub_child = tree.children(tree.root())[0].id;
    let mut log = Log::new();
    let out = tree
        .child_failed(a, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert_eq!(out, Outcome::Stopped { child: sub_child });
    assert_eq!(log, ["stop a (panic: boom)"]);
    assert!(!tree.contains(sub));
    assert_eq!(tree.owner(a), None);
    assert_eq!(tree.supervisor_count(), 1);
}

/// outer ── content ··· inner ── part
fn nested(inner_intensity: Intensity) -> (SupervisorTree<Log>, ChildId, SupId, ChildId) {
    let tree: SupervisorTree<Log> = SupervisorTree::new("outer", Strategy::OneForOne, lenient());
    let content = tree
        .add_child(tree.root(), "content", Restart::Permanent, leaf("content"))
        .unwrap();
    let inner = tree
        .add_linked_supervisor(content, "inner", Strategy::OneForOne, inner_intensity)
        .unwrap();
    let part = tree
        .add_child(inner, "part", Restart::Permanent, leaf("part"))
        .unwrap();
    (tree, content, inner, part)
}

#[test]
fn a_linked_supervisor_that_gives_up_takes_its_owner_down_with_the_same_reason() {
    let (tree, content, inner, part) = nested(Intensity::new(1, Duration::from_secs(1)));
    assert_eq!(tree.linked(content), vec![inner]);
    assert_eq!(tree.parent(inner), Some(tree.root()));
    let clock = VirtualClock::new();
    let mut log = Log::new();
    let out = tree
        .child_failed(part, boom(), clock.now(), &mut log)
        .unwrap();
    assert_eq!(
        out,
        Outcome::Restarted {
            by: inner,
            started: vec![part]
        }
    );
    log.clear();
    let out = tree
        .child_failed(part, boom(), clock.now(), &mut log)
        .unwrap();
    assert_eq!(
        log,
        [
            "stop part (panic: boom)",
            "stop content (panic: boom)",
            "start content"
        ]
    );
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![content]
        }
    );
    // The restarted owner links what it starts anew.
    assert!(!tree.contains(inner));
    assert!(tree.linked(content).is_empty());
    assert_eq!(tree.owner(part), None);
}

#[test]
fn what_a_leaf_started_stops_before_the_leaf() {
    let (tree, content, inner, _) = nested(lenient());
    let mut log = Log::new();
    tree.terminate_child(content, &mut log).unwrap();
    assert_eq!(log, ["stop part", "stop content"]);
    assert!(!tree.contains(inner));
    assert_eq!(tree.owner(content), None);
}

#[test]
fn a_child_that_cannot_start_is_tried_as_often_as_the_intensity_allows() {
    let tree: SupervisorTree<Log> = SupervisorTree::new(
        "root",
        Strategy::OneForOne,
        Intensity::new(2, Duration::from_secs(1)),
    );
    let spec = leaf("a");
    let a = tree
        .add_child(tree.root(), "a", Restart::Permanent, spec.clone())
        .unwrap();
    let mut log = Log::new();
    // Fails once, then starts.
    spec.fail.set(1);
    let out = tree
        .child_failed(a, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert_eq!(log, ["stop a (panic: boom)", "fail a", "start a"]);
    assert!(matches!(out, Outcome::Restarted { .. }));
    assert_eq!(tree.recent_restarts(tree.root()), 2);

    // Never starts: the root gives up with the start's fault.
    let tree: SupervisorTree<Log> = SupervisorTree::new(
        "root",
        Strategy::OneForOne,
        Intensity::new(2, Duration::from_secs(1)),
    );
    let spec = leaf("a");
    let a = tree
        .add_child(tree.root(), "a", Restart::Permanent, spec.clone())
        .unwrap();
    spec.fail.set(u32::MAX);
    log.clear();
    let out = tree
        .child_failed(a, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert_eq!(log, ["stop a (panic: boom)", "fail a", "fail a"]);
    assert_eq!(
        out,
        Outcome::GaveUp {
            sup: tree.root(),
            reason: ExitReason::Fault(Fault::Startup("a cannot start".into()))
        }
    );
}

#[test]
fn start_child_adds_only_what_started() {
    let tree: SupervisorTree<Log> = SupervisorTree::new("root", Strategy::OneForOne, lenient());
    let mut log = Log::new();
    let bad = leaf("bad");
    bad.fail.set(1);
    let err = tree
        .start_child(tree.root(), "bad", Restart::Permanent, bad, &mut log)
        .unwrap_err();
    assert_eq!(
        err,
        StartError::Fault(Fault::Startup("bad cannot start".into()))
    );
    assert!(tree.children(tree.root()).is_empty());
    let good = tree
        .start_child(
            tree.root(),
            "good",
            Restart::Permanent,
            leaf("good"),
            &mut log,
        )
        .unwrap();
    assert_eq!(log, ["fail bad", "start good"]);
    assert_eq!(tree.owner(good), Some(tree.root()));
}

#[test]
fn shutdown_stops_everything_last_first_and_restarts_nothing() {
    let (tree, content, _, _) = nested(lenient());
    tree.add_child(tree.root(), "late", Restart::Permanent, leaf("late"))
        .unwrap();
    let mut log = Log::new();
    tree.shutdown(&mut log);
    assert_eq!(log, ["stop late", "stop part", "stop content"]);
    assert!(tree.children(tree.root()).iter().all(|c| !c.running));
    // Stopping twice stops nothing twice.
    tree.shutdown(&mut log);
    assert_eq!(log.len(), 3);
    let _ = content;
}

#[test]
fn the_standard_tree_keeps_a_ui_failure_away_from_the_services() {
    let (tree, sups) = SupervisorTree::<Log>::standard();
    assert_eq!(tree.root(), sups.app);
    let names: Vec<String> = tree
        .children(sups.app)
        .iter()
        .map(|c| c.name.to_string())
        .collect();
    assert_eq!(names, ["ui", "services", "plugins"]);
    assert!(
        tree.children(sups.app)
            .iter()
            .all(|c| c.kind == ChildKind::Supervisor)
    );

    let page = tree
        .add_supervisor(
            sups.ui,
            "page",
            Restart::Permanent,
            Strategy::OneForOne,
            Intensity::new(0, Duration::from_secs(1)),
        )
        .unwrap();
    let content = tree
        .add_child(page, "content", Restart::Permanent, leaf("content"))
        .unwrap();
    let service = tree
        .add_child(
            sups.services,
            "service",
            Restart::Permanent,
            leaf("service"),
        )
        .unwrap();
    let mut log = Log::new();
    // `page` allows no restart: it gives up at once and `ui` restarts it.
    let out = tree
        .child_failed(content, boom(), VirtualClock::new().now(), &mut log)
        .unwrap();
    assert!(
        matches!(out, Outcome::Restarted { by, .. } if by == sups.ui),
        "{out:?}"
    );
    assert_eq!(log, ["stop content (panic: boom)", "start content"]);
    assert!(tree.children(sups.services)[0].running);
    let _ = service;

    let dump = tree.dump();
    assert!(dump.starts_with("app (OneForOne, 3 / 5s)\n  ui "), "{dump}");
    assert!(
        dump.contains("    service [Other(\"test\"), Permanent]"),
        "{dump}"
    );
}

/// A context that holds the tree, for children that call back into it.
struct World {
    log: Log,
    tree: Rc<SupervisorTree<World>>,
    clock: VirtualClock,
}

/// Reports the exit of another child from inside its own stop.
struct Dragging {
    name: &'static str,
    other: Cell<Option<ChildId>>,
    seen: RefCell<Vec<Outcome>>,
}

impl ChildSpec<World> for Dragging {
    fn kind(&self) -> ChildKind {
        ChildKind::Boundary
    }

    fn start(&self, w: &mut World) -> Result<(), Fault> {
        w.log.push(format!("start {}", self.name));
        Ok(())
    }

    fn stop(&self, w: &mut World, _reason: &ExitReason) {
        w.log.push(format!("stop {}", self.name));
        if let Some(other) = self.other.take() {
            let tree = w.tree.clone();
            let now = w.clock.now();
            let out = tree
                .child_exited(other, ExitReason::Fault(Fault::JsOutOfMemory), now, w)
                .unwrap();
            self.seen.borrow_mut().push(out);
        }
    }
}

#[test]
fn an_exit_reported_while_another_is_handled_is_handled_after_it() {
    let tree: Rc<SupervisorTree<World>> =
        Rc::new(SupervisorTree::new("root", Strategy::OneForOne, lenient()));
    let mk = |name| {
        Rc::new(Dragging {
            name,
            other: Cell::new(None),
            seen: RefCell::new(Vec::new()),
        })
    };
    let (sa, sb) = (mk("a"), mk("b"));
    let a = tree
        .add_child(tree.root(), "a", Restart::Permanent, sa.clone())
        .unwrap();
    let b = tree
        .add_child(tree.root(), "b", Restart::Permanent, sb.clone())
        .unwrap();
    sa.other.set(Some(b));
    let mut w = World {
        log: Log::new(),
        tree: tree.clone(),
        clock: VirtualClock::new(),
    };
    let now = w.clock.now();
    let out = tree.child_failed(a, boom(), now, &mut w).unwrap();
    assert_eq!(
        out,
        Outcome::Restarted {
            by: tree.root(),
            started: vec![a]
        }
    );
    assert_eq!(*sa.seen.borrow(), [Outcome::Queued]);
    // b exited by itself (not stopped) and is restarted after a.
    assert_eq!(w.log, ["stop a", "start a", "start b"]);
    assert_eq!(tree.recent_restarts(tree.root()), 2);
}

/// Links a supervisor with one child to itself every time it starts.
struct Nesting {
    me: Cell<Option<ChildId>>,
    made: Cell<u32>,
}

impl ChildSpec<World> for Nesting {
    fn kind(&self) -> ChildKind {
        ChildKind::Boundary
    }

    fn start(&self, w: &mut World) -> Result<(), Fault> {
        let me = self.me.get().expect("set before the first start");
        let tree = w.tree.clone();
        let inner = tree
            .add_linked_supervisor(me, "inner", Strategy::OneForOne, lenient())
            .map_err(|e| Fault::Startup(e.to_string()))?;
        tree.add_child(inner, "part", Restart::Permanent, leaf_w("part"))
            .map_err(|e| Fault::Startup(e.to_string()))?;
        self.made.set(self.made.get() + 1);
        w.log.push("start content".into());
        Ok(())
    }

    fn stop(&self, w: &mut World, _reason: &ExitReason) {
        w.log.push("stop content".into());
    }
}

struct LeafW(&'static str);

fn leaf_w(name: &'static str) -> Rc<LeafW> {
    Rc::new(LeafW(name))
}

impl ChildSpec<World> for LeafW {
    fn kind(&self) -> ChildKind {
        ChildKind::Boundary
    }

    fn start(&self, w: &mut World) -> Result<(), Fault> {
        w.log.push(format!("start {}", self.0));
        Ok(())
    }

    fn stop(&self, w: &mut World, _reason: &ExitReason) {
        w.log.push(format!("stop {}", self.0));
    }
}

#[test]
fn a_child_may_link_supervisors_while_it_starts() {
    let tree: Rc<SupervisorTree<World>> =
        Rc::new(SupervisorTree::new("root", Strategy::OneForOne, lenient()));
    let spec = Rc::new(Nesting {
        me: Cell::new(None),
        made: Cell::new(0),
    });
    let content = tree
        .add_child(tree.root(), "content", Restart::Permanent, spec.clone())
        .unwrap();
    spec.me.set(Some(content));
    let mut w = World {
        log: Log::new(),
        tree: tree.clone(),
        clock: VirtualClock::new(),
    };
    spec.start(&mut w).unwrap();
    assert_eq!(tree.linked(content).len(), 1);
    let first = tree.linked(content)[0];
    w.log.clear();
    let now = w.clock.now();
    tree.child_failed(content, boom(), now, &mut w).unwrap();
    // The old inner supervisor went with the old content; the new content
    // linked a new one.
    assert_eq!(w.log, ["stop part", "stop content", "start content"]);
    assert_eq!(spec.made.get(), 2);
    assert!(!tree.contains(first));
    assert_eq!(tree.linked(content).len(), 1);
    assert_eq!(tree.supervisor_count(), 2);
}

#[test]
fn linking_to_a_supervisor_is_refused() {
    let tree: SupervisorTree<Log> = SupervisorTree::new("root", Strategy::OneForOne, lenient());
    tree.add_supervisor(
        tree.root(),
        "sub",
        Restart::Permanent,
        Strategy::OneForOne,
        lenient(),
    )
    .unwrap();
    let sub_child = tree.children(tree.root())[0].id;
    assert_eq!(
        tree.add_linked_supervisor(sub_child, "x", Strategy::OneForOne, lenient()),
        Err(SupervisorError::NotALeaf(sub_child))
    );
    assert_eq!(
        tree.add_linked_supervisor(ChildId(999), "x", Strategy::OneForOne, lenient()),
        Err(SupervisorError::UnknownChild(ChildId(999)))
    );
}
