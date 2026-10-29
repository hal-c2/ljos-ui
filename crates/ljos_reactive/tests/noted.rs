//! `Graph::set_noted`: a property whose changes the note handler is told
//! of, once the change has spread, in place of an effect that reads it.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_reactive::{Graph, NodeId};

fn noting(g: &Graph<i64>) -> Rc<RefCell<Vec<NodeId>>> {
    let told = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::clone(&told);
    g.set_note_handler(move |n| t.borrow_mut().push(n));
    told
}

#[test]
fn a_value_written_is_told_once_it_changes() {
    let g: Graph<i64> = Graph::new();
    let told = noting(&g);
    let p = g.property(g.root(), 0).unwrap();
    g.set_noted(p).unwrap();
    g.set(p, 1);
    assert_eq!(told.take(), [p]);
    g.set(p, 1);
    assert!(told.take().is_empty(), "the value it had is no change");
}

#[test]
fn a_batch_tells_once_when_it_ends() {
    let g: Graph<i64> = Graph::new();
    let told = noting(&g);
    let p = g.property(g.root(), 0).unwrap();
    g.set_noted(p).unwrap();
    g.batch(|| {
        g.set(p, 1);
        g.set(p, 2);
        assert!(told.borrow().is_empty(), "told when the change has spread");
    });
    assert_eq!(told.take(), [p]);
}

#[test]
fn a_binding_is_told_before_it_runs_and_runs_when_read() {
    let g: Graph<i64> = Graph::new();
    let told = noting(&g);
    let r = g.root();
    let a = g.property(r, 1).unwrap();
    let runs = Rc::new(RefCell::new(0));
    let n = Rc::clone(&runs);
    let b = g
        .binding(r, 0, move |g| {
            *n.borrow_mut() += 1;
            g.get(a) * 10
        })
        .unwrap();
    g.set_noted(b).unwrap();
    assert_eq!(g.get(b), 10);
    g.set(a, 2);
    g.set(a, 3);
    // Told on the first change; not read since, so not told again.
    assert_eq!(told.take(), [b]);
    assert_eq!(*runs.borrow(), 1, "not run for being told");
    assert_eq!(g.get(b), 30);
    assert_eq!(*runs.borrow(), 2);
    assert!(told.take().is_empty(), "its own result is not told again");
    g.set(a, 4);
    assert_eq!(told.take(), [b]);
}

#[test]
fn a_property_not_noted_is_not_told() {
    let g: Graph<i64> = Graph::new();
    let told = noting(&g);
    let p = g.property(g.root(), 0).unwrap();
    g.set(p, 1);
    assert!(told.take().is_empty());
}

#[test]
fn a_nested_write_runs_its_watchers_before_the_noted_propertys_watchers() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let parent = g.property(r, 0).unwrap();
    let child = g.property(r, 1).unwrap();
    let writes = Rc::clone(&g);
    g.set_note_handler(move |n| {
        let _hold = writes.hold_observers(n);
        if n == parent {
            writes.set(child, 2);
        }
    });
    g.set_noted(parent).unwrap();
    g.set_noted(child).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for (property, name) in [(parent, "parent"), (child, "child")] {
        let log = Rc::clone(&log);
        g.watch(r, property, move |_, _| log.borrow_mut().push(name))
            .unwrap();
    }

    g.set(parent, 1);

    assert_eq!(log.take(), ["child", "parent"]);
}

#[test]
fn a_second_hold_on_a_held_property_does_not_keep_its_watchers_held() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let writes = Rc::clone(&g);
    g.set_note_handler(move |n| {
        let _outer = writes.hold_observers(n);
        assert!(writes.hold_observers(n).is_none());
    });
    g.set_noted(p).unwrap();
    let told = Rc::new(RefCell::new(0));
    let t = Rc::clone(&told);
    g.watch(r, p, move |_, _| *t.borrow_mut() += 1).unwrap();

    g.set(p, 1);
    g.set(p, 2);

    assert_eq!(*told.borrow(), 2);
}
