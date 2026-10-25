//! [`Graph::watch_ahead`]: a handler that runs before what depends on the
//! property it watches, as a QML object's own `onXChanged` does in Qt.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_reactive::Graph;

type Log = Rc<RefCell<Vec<(i64, i64)>>>;

fn log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

/// `open` → `target: open ? 540 : 0` → `shown: target`, a handler on
/// `target` that reads `shown` (issue #116).
#[test]
fn a_handler_ahead_reads_a_binding_on_its_property_as_it_was() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let open = g.property(r, 0).unwrap();
    let target = g.binding(r, 0, move |g| g.get(open) * 540).unwrap();
    let shown = g.binding(r, 0, move |g| g.get(target)).unwrap();
    assert_eq!(g.get(shown), 0);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, target, move |g, v| {
        s.borrow_mut().push((*v, g.get(shown)))
    })
    .unwrap();
    g.set(open, 1);
    g.set(open, 0);
    g.set(open, 1);
    assert_eq!(*seen.borrow(), vec![(540, 0), (0, 540), (540, 0)]);
    assert_eq!(g.get(shown), 540);
}

#[test]
fn a_plain_watch_reads_the_bindings_brought_up_to_date() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
    assert_eq!(g.get(b), 1);
    let seen = log();
    let s = seen.clone();
    g.watch(r, a, move |g, v| s.borrow_mut().push((*v, g.get(b))))
        .unwrap();
    g.set(a, 5);
    assert_eq!(*seen.borrow(), vec![(5, 6)]);
}

/// `a` → `b: a + 1` → `c: b * 10`: the handler of `b` sees `b` new and
/// `c` as it was; that of `a` sees both as they were.
#[test]
fn along_a_chain_each_handler_ahead_sees_its_own_property_new() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
    let c = g.binding(r, 0, move |g| g.get(b) * 10).unwrap();
    assert_eq!(g.get(c), 10);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, a, move |g, _| s.borrow_mut().push((g.get(b), g.get(c))))
        .unwrap();
    let s = seen.clone();
    g.watch_ahead(r, b, move |g, v| s.borrow_mut().push((*v, g.get(c))))
        .unwrap();
    g.set(a, 1);
    assert_eq!(*seen.borrow(), vec![(1, 10), (2, 10)]);
    assert_eq!(g.get(c), 20);
}

/// What the handler writes is not the change it was told of: a binding
/// that write reaches is read brought up to date.
#[test]
fn a_binding_the_handlers_own_write_reaches_is_read_new() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let x = g.property(r, 0).unwrap();
    let y = g.binding(r, 0, move |g| g.get(x) + 1).unwrap();
    assert_eq!(g.get(y), 1);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, a, move |g, v| {
        g.set(x, *v * 2);
        s.borrow_mut().push((*v, g.get(y)));
    })
    .unwrap();
    g.set(a, 3);
    assert_eq!(*seen.borrow(), vec![(3, 7)]);
}

/// A write in a handler ahead flushes, and the other handlers waiting run
/// in it: those are not ahead, and read the bindings new.
#[test]
fn a_plain_watch_run_inside_a_handler_ahead_reads_new() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) + 1).unwrap();
    let other = g.property(r, 0).unwrap();
    assert_eq!(g.get(b), 1);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, a, move |g, v| {
        s.borrow_mut().push((*v, g.get(b)));
        g.set(other, *v);
    })
    .unwrap();
    let s = seen.clone();
    g.watch(r, a, move |g, v| s.borrow_mut().push((*v, g.get(b))))
        .unwrap();
    g.set(a, 5);
    assert_eq!(*seen.borrow(), vec![(5, 1), (5, 6)]);
}

/// The feature's shape: handlers ahead on `open`, `target` and `shown`
/// that write, and a watch behind on `target`, which runs last.
#[test]
fn a_watch_behind_runs_after_the_handlers_ahead_below_it() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let open = g.property(r, 0).unwrap();
    let target = g.binding(r, 0, move |g| g.get(open) * 540).unwrap();
    let shown = g.binding(r, 0, move |g| g.get(target)).unwrap();
    let chain = g.binding(r, 0, move |g| g.get(shown) + 1).unwrap();
    let out = g.property(r, 0).unwrap();
    assert_eq!(g.get(chain), 1);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, open, move |g, _| {
        s.borrow_mut().push((g.get(target), g.get(shown)));
        g.set(out, g.get_untracked(out) + 1);
    })
    .unwrap();
    let s = seen.clone();
    g.watch_ahead(r, target, move |g, _| {
        s.borrow_mut().push((g.get(shown), g.get(chain)));
        g.set(out, g.get_untracked(out) + 1);
    })
    .unwrap();
    let s = seen.clone();
    g.watch_ahead(r, shown, move |g, v| {
        s.borrow_mut().push((*v, g.get(chain)));
        g.set(out, g.get_untracked(out) + 1);
    })
    .unwrap();
    let s = seen.clone();
    g.watch_behind(r, target, move |g, _| {
        s.borrow_mut().push((g.get(shown), g.get(chain)));
    })
    .unwrap();
    g.set(open, 1);
    assert_eq!(*seen.borrow(), vec![(0, 0), (0, 1), (540, 1), (540, 541)],);
}

/// A binding nothing has read yet is evaluated by the first handler that
/// reads it, against what that handler sees; a change of a binding it
/// read, brought up to date later, is still the same change to the next.
#[test]
fn a_binding_first_read_in_a_handler_ahead_stays_part_of_the_change() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let a = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(a) * 10).unwrap();
    let c = g.binding(r, 0, move |g| g.get(b) + 1).unwrap();
    assert_eq!(g.get(b), 0);
    let seen = log();
    let s = seen.clone();
    g.watch_ahead(r, a, move |g, v| s.borrow_mut().push((*v, g.get(c))))
        .unwrap();
    let s = seen.clone();
    g.watch_ahead(r, b, move |g, v| s.borrow_mut().push((*v, g.get(c))))
        .unwrap();
    g.set(a, 1);
    assert_eq!(*seen.borrow(), vec![(1, 1), (10, 1)]);
    assert_eq!(g.get(c), 11);
}
