//! A run that a passing fault stops (a full heap, an interrupt) keeps what
//! it depended on before, so it runs again once the fault has passed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_reactive::{Graph, Reader};

#[test]
fn a_run_cut_short_keeps_the_sources_it_did_not_get_to_read() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    let fail = Rc::new(Cell::new(false));
    let last = Rc::new(Cell::new(0));
    let (f, l) = (fail.clone(), last.clone());
    // As the engine's binding wrapper does: a run that a fault stops keeps
    // the last value.
    let sum = g
        .binding(r, 0, move |g| {
            let x = g.get(a);
            if f.get() {
                g.cut_short();
                return l.get();
            }
            let v = x + g.get(b);
            l.set(v);
            v
        })
        .unwrap();
    assert_eq!(g.get(sum), 3);

    fail.set(true);
    g.set(a, 10);
    assert_eq!(g.get(sum), 3, "the run was cut short before it read b");

    fail.set(false);
    g.set(b, 20);
    assert_eq!(
        g.get(sum),
        30,
        "b, never read by the cut-short run, still wakes it"
    );
}

#[test]
fn a_run_that_finishes_drops_what_a_cut_short_one_kept() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    let mode = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let (m, n) = (mode.clone(), runs.clone());
    let v = g
        .binding(r, 0, move |g| {
            n.set(n.get() + 1);
            match m.get() {
                0 => g.get(a) + g.get(b),
                1 => {
                    g.get(a);
                    g.cut_short();
                    0
                }
                _ => g.get(a),
            }
        })
        .unwrap();
    assert_eq!(g.get(v), 3);
    mode.set(1);
    g.set(a, 5);
    g.get(v);
    mode.set(2);
    g.set(a, 6);
    assert_eq!(g.get(v), 6);
    let before = runs.get();
    g.set(b, 7);
    assert_eq!(g.get(v), 6);
    assert_eq!(runs.get(), before, "b was dropped by the run that finished");
}

#[test]
fn cut_short_marks_the_run_past_an_untrack() {
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    let fail = Rc::new(Cell::new(false));
    let f = fail.clone();
    let v = g
        .binding(r, 0, move |g| {
            let x = g.get(a);
            if f.get() {
                g.untrack(|| g.cut_short());
                return -1;
            }
            x + g.get(b)
        })
        .unwrap();
    assert_eq!(g.get(v), 3);
    fail.set(true);
    g.set(a, 2);
    assert_eq!(g.get(v), -1);
    fail.set(false);
    g.set(b, 5);
    assert_eq!(g.get(v), 7);
}

#[test]
fn a_run_cut_short_keeps_what_its_last_whole_run_read_without_a_node() {
    // A value nothing has written yet has no node: its caller keeps the
    // readers ([`Graph::reader`]) and makes the node only when it changes.
    // Two cut-short runs in a row still leave the whole run's read standing.
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let readers: Rc<RefCell<Vec<Reader>>> = Rc::default();
    let unwritten = Rc::new(Cell::new(2));
    let fail = Rc::new(Cell::new(false));
    let last = Rc::new(Cell::new(0));
    let (rs, u, f, l) = (
        readers.clone(),
        unwritten.clone(),
        fail.clone(),
        last.clone(),
    );
    let sum = g
        .binding(r, 0, move |g| {
            let x = g.get(a);
            if f.get() {
                g.cut_short();
                return l.get();
            }
            rs.borrow_mut().extend(g.reader());
            let v = x + u.get();
            l.set(v);
            v
        })
        .unwrap();
    assert_eq!(g.get(sum), 3);

    fail.set(true);
    g.set(a, 10);
    assert_eq!(g.get(sum), 3);
    g.set(a, 11);
    assert_eq!(g.get(sum), 3);

    fail.set(false);
    unwritten.set(20);
    let b = g.property(r, 2).unwrap();
    let kept: Vec<Reader> = readers.borrow().clone();
    assert!(kept.iter().all(|k| g.is_current(*k)));
    g.adopt_readers(b, kept);
    g.set(b, 20);
    assert_eq!(g.get(sum), 31, "the unwritten value still wakes it");
}

#[test]
fn a_read_without_a_node_first_made_by_a_cut_short_run_survives_the_next_one() {
    // The read is made only in the cut-short runs: the run before them,
    // which finished, never made it.
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let readers: Rc<RefCell<Vec<Reader>>> = Rc::default();
    let mode = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let (rs, m, n) = (readers.clone(), mode.clone(), runs.clone());
    let v = g
        .binding(r, 0, move |g| {
            n.set(n.get() + 1);
            let x = g.get(a);
            match m.get() {
                // Reads the unwritten value, then is cut short.
                1 => {
                    rs.borrow_mut().extend(g.reader());
                    g.cut_short();
                }
                // Cut short before it reads it.
                2 => g.cut_short(),
                _ => {}
            }
            x
        })
        .unwrap();
    assert_eq!(g.get(v), 1);
    mode.set(1);
    g.set(a, 2);
    assert_eq!(g.get(v), 2);
    mode.set(2);
    g.set(a, 3);
    assert_eq!(g.get(v), 3);

    let kept: Vec<Reader> = readers.borrow().clone();
    assert_eq!(kept.len(), 1);
    assert!(kept.iter().all(|k| g.is_current(*k)), "{kept:?}");
    mode.set(0);
    let c = g.property(r, 0).unwrap();
    g.adopt_readers(c, kept);
    let before = runs.get();
    g.set(c, 1);
    g.get(v);
    assert_eq!(runs.get(), before + 1, "the unwritten value still wakes it");
}

#[test]
fn a_binding_cut_short_on_its_first_run_wakes_only_on_what_it_read() {
    // It has no run before to keep the sources of: what it never got to
    // read cannot wake it. Running it again once the fault has passed is
    // up to whoever was told of the fault.
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let b = g.property(r, 2).unwrap();
    let fail = Rc::new(Cell::new(true));
    let runs = Rc::new(Cell::new(0));
    let (f, n) = (fail.clone(), runs.clone());
    let v = g
        .binding(r, 0, move |g| {
            n.set(n.get() + 1);
            let x = g.get(a);
            if f.get() {
                g.cut_short();
                return -1;
            }
            x + g.get(b)
        })
        .unwrap();
    assert_eq!(g.get(v), -1);
    fail.set(false);
    g.set(b, 5);
    assert_eq!(g.get(v), -1, "b was never read");
    assert_eq!(runs.get(), 1);
    g.set(a, 2);
    assert_eq!(g.get(v), 7, "a was");
}

#[test]
fn a_read_without_a_node_by_a_run_that_panicked_between_cut_short_ones_wakes_it_once() {
    // A panicked run never finishes, so its reads count for nothing on
    // their own. Between cut-short runs, though, a reader counts by its
    // stamp lying in the kept range, and a panicked run's stamp lies there
    // too: what it read without a node wakes the binding once. That run is
    // whole, so it runs on what holds now and drops the read it does not
    // make; the cost is one run, never a wrong value.
    let g = Graph::new();
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let readers: Rc<RefCell<Vec<Reader>>> = Rc::default();
    let mode = Rc::new(Cell::new(0));
    let runs = Rc::new(Cell::new(0));
    let (rs, m, n) = (readers.clone(), mode.clone(), runs.clone());
    let v = g
        .binding(r, 0, move |g| {
            n.set(n.get() + 1);
            let x = g.get(a);
            match m.get() {
                2 => g.cut_short(),
                3 => {
                    rs.borrow_mut().extend(g.reader());
                    panic!("the binding failed");
                }
                _ => {}
            }
            x
        })
        .unwrap();
    assert_eq!(g.get(v), 1);
    mode.set(2);
    g.set(a, 2);
    assert_eq!(g.get(v), 2);
    mode.set(3);
    g.set(a, 3);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.get(v)));
    assert!(res.is_err());
    mode.set(2);
    assert_eq!(g.get(v), 3, "retried, and cut short again");
    g.check_invariants().unwrap();

    let read: Vec<Reader> = readers.borrow().clone();
    assert_eq!(read.len(), 1);
    assert!(
        read.iter().all(|k| g.is_current(*k)),
        "the panicked run's stamp lies in the kept range: {read:?}"
    );
    mode.set(0);
    let c = g.property(r, 0).unwrap();
    g.adopt_readers(c, read.clone());
    let before = runs.get();
    g.set(c, 1);
    assert_eq!(g.get(v), 3, "the extra run gives what holds now");
    assert_eq!(runs.get(), before + 1, "it woke the binding once");

    assert!(
        read.iter().all(|k| !g.is_current(*k)),
        "the whole run ended the kept range"
    );
    g.set(c, 2);
    g.get(v);
    assert_eq!(
        runs.get(),
        before + 1,
        "and dropped the read it did not make"
    );
    g.check_invariants().unwrap();
}
