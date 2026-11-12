//! `Graph::lift_batches`: work done in steps inside a batch, over a queue
//! of its own, runs as it would with no batch open, and what the batch
//! held back before still waits for it unless that work reaches it again.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_reactive::Graph;

type Log = Rc<RefCell<Vec<&'static str>>>;

fn watching(
    g: &Graph<i64>,
    scope: ljos_reactive::ScopeId,
    name: &'static str,
    log: &Log,
) -> ljos_reactive::NodeId {
    let p = g.property(scope, 0).unwrap();
    let l = Rc::clone(log);
    g.effect(scope, move |g| {
        g.get(p);
        l.borrow_mut().push(name);
    })
    .unwrap();
    p
}

#[test]
fn what_is_queued_over_the_lift_runs_at_once_and_what_came_before_waits() {
    let g: Graph<i64> = Graph::new();
    let log = Log::default();
    let before = watching(&g, g.root(), "before", &log);
    let own = watching(&g, g.root(), "own", &log);
    log.take();
    g.batch(|| {
        g.set(before, 1);
        {
            let queue = g.own_queue(g.changes_so_far());
            let _lift = g.lift_batches(&queue);
            assert!(!g.batching());
            g.set(own, 1);
            assert_eq!(log.take(), ["own"], "a write flushes");
            g.batch(|| {
                g.set(own, 2);
                g.set(own, 3);
                assert!(log.borrow().is_empty());
            });
            assert_eq!(log.take(), ["own"], "a batch flushes as it ends");
        }
        assert!(g.batching(), "the batch goes on");
        g.set(own, 4);
        assert!(log.borrow().is_empty(), "held back again");
    });
    let mut ran = log.take();
    ran.sort_unstable();
    assert_eq!(ran, ["before", "own"]);
}

#[test]
fn what_came_before_and_is_reached_again_over_the_lift_runs_with_it() {
    let g: Graph<i64> = Graph::new();
    let log = Log::default();
    let before = watching(&g, g.root(), "before", &log);
    log.take();
    g.batch(|| {
        g.set(before, 1);
        let queue = g.own_queue(g.changes_so_far());
        let _lift = g.lift_batches(&queue);
        g.set(before, 2);
        assert_eq!(log.take(), ["before"], "as with no batch open");
    });
    assert!(log.borrow().is_empty(), "not again when the batch ends");
}

#[test]
fn a_scope_asked_to_freeze_in_the_batch_waits_for_it() {
    let g: Graph<i64> = Graph::new();
    let log = Log::default();
    let page = g.create_scope(g.root()).unwrap();
    let shown = watching(&g, page, "page", &log);
    let own = watching(&g, g.root(), "own", &log);
    log.take();
    g.batch(|| {
        g.set_frozen(page, true).unwrap();
        let queue = g.own_queue(g.changes_so_far());
        let _lift = g.lift_batches(&queue);
        g.set(own, 1);
        g.set(shown, 1);
        assert_eq!(
            log.take(),
            ["own", "page"],
            "not frozen before the batch ends"
        );
    });
    g.set(shown, 2);
    assert!(log.borrow().is_empty(), "frozen once it has");
}
