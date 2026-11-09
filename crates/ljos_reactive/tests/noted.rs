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

/// A handler ahead on `saved` writes `working`, which two noted bindings
/// read. The hook told first opens a queue of its own for the change and
/// writes in it, which flushes: the other hook, told of the same change,
/// is told once that queue is done and reads its own property new
/// (issue #237).
#[test]
fn a_hook_a_waiting_change_reached_is_told_after_the_queue_it_waits_on() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let plans = g.property(r, 0).unwrap();
    let saved = g.binding(r, 0, move |g| g.get(plans)).unwrap();
    let working = g.property(r, 0).unwrap();
    let shown = g.binding(r, 0, move |g| g.get(working)).unwrap();
    let enabled = g.binding(r, 0, move |g| g.get(working)).unwrap();
    let other = g.property(r, 0).unwrap();
    assert_eq!((g.get(shown), g.get(enabled)), (0, 0));
    g.watch_ahead(r, saved, move |g, v| g.set(working, *v))
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (hooks, s) = (Rc::clone(&g), Rc::clone(&seen));
    g.set_note_handler(move |n| {
        s.borrow_mut().push((n, hooks.get(n)));
        let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(1));
        let o = hooks.untrack(|| hooks.get(other));
        hooks.set(other, o + 1);
    });
    g.set_noted(shown).unwrap();
    g.set_noted(enabled).unwrap();

    g.set(plans, 1);

    let seen = seen.take();
    assert_eq!(seen.len(), 2, "each hook told once: {seen:?}");
    assert!(
        seen.iter().all(|&(_, v)| v == 1),
        "each hook reads its own change: {seen:?}"
    );
}

/// A batch writes `p` then `q`, both noted. `q`'s hook, told first, opens
/// a queue of its own and writes `p` again. That change is made inside
/// the queue, so `p`'s hook, waiting below it, is told inside it too,
/// before `p`'s watcher runs there: the watcher reads what the hook keeps
/// up to date as of the value it is given, and the hook is told once.
#[test]
fn a_hook_a_change_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    // What `p`'s hook keeps: the value of `p` it was last told of.
    let kept = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, s) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, p, move |_, v| s.borrow_mut().push((*v, *k.borrow())))
        .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == p {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(p));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(p, 2);
        }
    });
    g.set_noted(p).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(p, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, p], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(2, 2)],
        "the watcher reads what the hook keeps as of the value it reads"
    );
}

/// [`a_hook_a_change_inside_the_queue_reaches_is_told_inside_it`] for a
/// told effect: one waiting to be told of `p` is told inside the queue a
/// write of `p` is made in, before the watcher that write sets off runs,
/// and once.
#[test]
fn a_told_effect_a_change_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&order);
    g.effect_told(
        r,
        move |g| {
            g.get(p);
        },
        move || o.borrow_mut().push("told"),
    )
    .unwrap();
    let o = Rc::clone(&order);
    g.watch(r, p, move |_, _| o.borrow_mut().push("watched"))
        .unwrap();
    let hooks = Rc::clone(&g);
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(p, 2);
        }
    });
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(p, 1);
        g.set(q, 1);
    });

    assert_eq!(
        *order.borrow(),
        ["told", "watched"],
        "told before the effects of the change, once"
    );
}

/// What a change made before the queue opened reached stays below it: a
/// hook inside the queue writes nothing that reaches `p`, so `p`'s hook
/// is told when the queue drops, after the watcher the queue runs.
#[test]
fn a_hook_only_a_waiting_change_reached_waits_for_the_queue() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let other = g.property(r, 0).unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&order);
    g.watch(r, other, move |_, _| o.borrow_mut().push("other watched"))
        .unwrap();
    let (hooks, o) = (Rc::clone(&g), Rc::clone(&order));
    g.set_note_handler(move |n| {
        if n == p {
            o.borrow_mut().push("p told");
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(1));
            hooks.set(other, 1);
        }
    });
    g.set_noted(p).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(p, 1);
        g.set(q, 1);
    });

    assert_eq!(*order.borrow(), ["other watched", "p told"]);
}

/// [`a_hook_a_change_inside_the_queue_reaches_is_told_inside_it`] for a
/// noted binding the batch left waiting to be checked: `b` reads `m`,
/// which reads `s`, and `t`. The batch's write of `s` reaches `b` through
/// `m`, so it waits to be checked, noted below the queue `q`'s hook opens.
/// The write of `t` inside that queue makes it dirty: its hook is told
/// inside the queue all the same, before `t`'s watcher runs there and
/// reads `b` with what the hook keeps of it.
#[test]
fn a_hook_a_change_inside_the_queue_makes_dirty_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let t = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let m = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let b = g.binding(r, 0, move |g| g.get(m) + 10 * g.get(t)).unwrap();
    assert_eq!(g.get(b), 0);
    // What `b`'s hook keeps: the value of `b` it was last told of.
    let kept = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, t, move |g, _| {
        let now = g.untrack(|| g.get(b));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == b {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(b));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(t, 2);
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, b], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(21, 21)],
        "the watcher reads what the hook keeps as of the value it reads"
    );
}

/// Many noted bindings of one source wait below the queue a hook opens,
/// and a write inside it changes that source again: each hook is lifted
/// by looking at its own entry, not by searching the ones below the
/// queue, so lifting them all costs as many steps as there are hooks.
#[test]
fn lifting_many_waiting_hooks_looks_at_each_once() {
    const HOOKS: usize = 1000;
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let source = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let bound: Vec<_> = (0..HOOKS)
        .map(|_| g.binding(r, 0, move |g| g.get(source)).unwrap())
        .collect();
    for &b in &bound {
        g.get(b);
        g.set_noted(b).unwrap();
    }
    let told = Rc::new(RefCell::new(0));
    let (hooks, t) = (Rc::clone(&g), Rc::clone(&told));
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(source, 2);
        } else {
            *t.borrow_mut() += 1;
        }
    });
    g.set_noted(q).unwrap();
    let before = g.stats().lift_steps;

    g.batch(|| {
        g.set(source, 1);
        g.set(q, 1);
    });

    assert_eq!(*told.borrow(), HOOKS, "each hook told once");
    let steps = g.stats().lift_steps - before;
    assert!(steps <= HOOKS as u64, "{steps} steps to lift {HOOKS} hooks");
}

/// [`lifting_many_waiting_hooks_looks_at_each_once`] for told effects.
#[test]
fn lifting_many_waiting_told_effects_looks_at_each_once() {
    const TOLD: usize = 1000;
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let source = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let told = Rc::new(RefCell::new(0));
    for _ in 0..TOLD {
        let t = Rc::clone(&told);
        g.effect_told(
            r,
            move |g| {
                g.get(source);
            },
            move || *t.borrow_mut() += 1,
        )
        .unwrap();
    }
    let hooks = Rc::clone(&g);
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(source, 2);
        }
    });
    g.set_noted(q).unwrap();
    let before = g.stats().lift_steps;

    g.batch(|| {
        g.set(source, 1);
        g.set(q, 1);
    });

    assert_eq!(*told.borrow(), TOLD, "each told once");
    let steps = g.stats().lift_steps - before;
    assert!(steps <= TOLD as u64, "{steps} steps to lift {TOLD} told");
}

/// Only a property has a hook: what keeps its place in the waiting list
/// is the property's.
#[test]
fn an_effect_is_not_noted() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let e = g.effect(r, |_| {}).unwrap();
    assert_eq!(
        g.set_noted(e),
        Err(ljos_reactive::ReactiveError::NotAProperty(e))
    );
}

/// [`a_hook_a_change_inside_the_queue_reaches_is_told_inside_it`] for a
/// noted binding behind one the batch left dirty: `b` reads `m`, which
/// reads `s`. The batch's write of `s` makes `m` dirty and leaves `b`
/// waiting to be checked, noted below the queue `q`'s hook opens. The
/// write of `s` inside that queue stops at `m`, already dirty, yet it
/// reaches `b` all the same: its hook is told inside the queue, before
/// `s`'s watcher runs there and reads `b` with what the hook keeps of it.
#[test]
fn a_hook_behind_a_dirty_binding_a_change_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let m = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let b = g.binding(r, 0, move |g| g.get(m)).unwrap();
    assert_eq!(g.get(b), 0);
    // What `b`'s hook keeps: the value of `b` it was last told of.
    let kept = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, s, move |g, _| {
        let now = g.untrack(|| g.get(b));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == b {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(b));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(s, 2);
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, b], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(2, 2)],
        "the watcher reads what the hook keeps as of the value it reads"
    );
}

/// [`a_hook_behind_a_dirty_binding_a_change_inside_the_queue_reaches_is_told_inside_it`]
/// for a told effect that reads `m`: the write of `s` inside the queue
/// stops at `m`, already dirty, and the effect is told inside the queue
/// all the same, before `s`'s watcher runs there, and once.
#[test]
fn a_told_effect_behind_a_dirty_binding_a_change_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let m = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&order);
    g.effect_told(
        r,
        move |g| {
            g.get(m);
        },
        move || o.borrow_mut().push("told"),
    )
    .unwrap();
    let o = Rc::clone(&order);
    g.watch(r, s, move |_, _| o.borrow_mut().push("watched"))
        .unwrap();
    let hooks = Rc::clone(&g);
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(s, 2);
        }
    });
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(
        *order.borrow(),
        ["told", "watched"],
        "told before the effects of the change, once"
    );
}

/// A change made inside the queue that finds a binding dirty again and
/// again goes past it once: `m` fans out to `FAN` bindings that `b`
/// reads, and `q`'s hook writes `s` `WRITES` times inside its queue.
/// `x`'s hook waits below the queue all the while, so there is always
/// something left to lift, and only having gone past `m` already stops
/// each walk.
#[test]
fn lifting_past_a_dirty_binding_goes_past_it_once_in_a_queue() {
    const FAN: usize = 100;
    const WRITES: i64 = 100;
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let x = g.property(r, 0).unwrap();
    let m = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let fan: Vec<_> = (0..FAN)
        .map(|_| g.binding(r, 0, move |g| g.get(m)).unwrap())
        .collect();
    let b = g
        .binding(r, 0, move |g| fan.iter().map(|&k| g.get(k)).sum())
        .unwrap();
    assert_eq!(g.get(b), 0);
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, h) = (Rc::clone(&g), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            for w in 2..2 + WRITES {
                hooks.set(s, w);
            }
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();
    g.set_noted(x).unwrap();
    let before = g.stats().lift_steps;

    g.batch(|| {
        g.set(s, 1);
        g.set(x, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, b, x], "each hook told once");
    let steps = g.stats().lift_steps - before;
    assert!(
        steps <= 3 * FAN as u64,
        "{steps} steps to lift one hook past {FAN} bindings {WRITES} times"
    );
    assert_eq!(g.get(b), (FAN as i64) * (WRITES + 1));
}

/// Outside an own queue nothing waits below a floor: a change that finds
/// a binding dirty again and again looks for nothing behind it.
#[test]
fn a_change_with_nothing_waiting_below_a_queue_looks_for_nothing() {
    const FAN: usize = 100;
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let m = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let fan: Vec<_> = (0..FAN)
        .map(|_| g.binding(r, 0, move |g| g.get(m)).unwrap())
        .collect();
    for &k in &fan {
        g.get(k);
        g.set_noted(k).unwrap();
    }
    let before = g.stats().lift_steps;

    g.batch(|| {
        for w in 1..=100 {
            g.set(s, w);
        }
    });

    assert_eq!(g.stats().lift_steps - before, 0);
}

/// [`a_hook_a_change_inside_the_queue_reaches_is_told_inside_it`] for a
/// disposal: `b` reads `s` and `d`. The batch's write of `s` leaves `b`
/// dirty, noted below the queue `q`'s hook opens for `q`'s own change. The
/// disposal of `d` inside it is a change of its own, made inside, and not
/// the one the queue waits on: it reaches `b`, whose hook is told inside
/// the queue, before the effect that read `d` runs there and reads `b`
/// with what the hook keeps of it.
#[test]
fn a_hook_a_disposal_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let d = g.property(r, 5).unwrap();
    let q = g.property(r, 0).unwrap();
    let b = g
        .binding(r, 0, move |g| g.get(s) + g.try_get(d).unwrap_or(0))
        .unwrap();
    assert_eq!(g.get(b), 5);
    // What `b`'s hook keeps: the value of `b` it was last told of.
    let kept = Rc::new(RefCell::new(5));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.effect(r, move |g| {
        g.try_get(d);
        let now = g.untrack(|| g.get(b));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    seen.take();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == b {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(b));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(1));
            hooks.dispose(d).unwrap();
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, b], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(1, 1)],
        "the effect reads what the hook keeps as of the value it reads"
    );
}

/// [`a_hook_a_disposal_inside_the_queue_reaches_is_told_inside_it`] for a
/// told effect that reads `b`: the disposal of `d` reaches it through
/// `b`, already dirty, and it is told inside the queue, before the effect
/// that read `d` runs there, and once.
#[test]
fn a_told_effect_a_disposal_inside_the_queue_reaches_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let d = g.property(r, 5).unwrap();
    let q = g.property(r, 0).unwrap();
    let b = g
        .binding(r, 0, move |g| g.get(s) + g.try_get(d).unwrap_or(0))
        .unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&order);
    g.effect_told(
        r,
        move |g| {
            g.get(b);
        },
        move || o.borrow_mut().push("told"),
    )
    .unwrap();
    let o = Rc::clone(&order);
    g.effect(r, move |g| {
        g.try_get(d);
        o.borrow_mut().push("ran");
    })
    .unwrap();
    order.take();
    let hooks = Rc::clone(&g);
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(1));
            hooks.dispose(d).unwrap();
        }
    });
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(
        *order.borrow(),
        ["told", "ran"],
        "told before the effects of the disposal, once"
    );
}

/// [`a_hook_behind_a_dirty_binding_a_change_inside_the_queue_reaches_is_told_inside_it`]
/// for a binding the change makes dirty that was waiting to be checked:
/// `b` reads `m`, which reads `a`, which reads `s`, and `m` also reads
/// `t`. The batch's write of `s` makes `a` dirty and leaves `m` and `b`
/// waiting to be checked, `b` noted below the queue `q`'s hook opens. The
/// write of `t` inside that queue makes `m` dirty and stops there, as
/// `b` waits to be checked already, yet it reaches `b` all the same: its
/// hook is told inside the queue, before `t`'s watcher runs there and
/// reads `b` with what the hook keeps of it.
#[test]
fn a_hook_behind_a_binding_a_change_inside_the_queue_makes_dirty_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let t = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let a = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let m = g.binding(r, 0, move |g| g.get(a) + 10 * g.get(t)).unwrap();
    let b = g.binding(r, 0, move |g| g.get(m)).unwrap();
    assert_eq!(g.get(b), 0);
    // What `b`'s hook keeps: the value of `b` it was last told of.
    let kept = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, t, move |g, _| {
        let now = g.untrack(|| g.get(b));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == b {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(b));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(t, 2);
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, b], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(21, 21)],
        "the watcher reads what the hook keeps as of the value it reads"
    );
}

/// [`a_hook_behind_a_binding_a_change_inside_the_queue_makes_dirty_is_told_inside_it`]
/// for a told effect that reads `b`: the write of `t` inside the queue
/// stops at `m`, which it makes dirty, and the effect is told inside the
/// queue all the same, before `t`'s watcher runs there, and once.
#[test]
fn a_told_effect_behind_a_binding_a_change_inside_the_queue_makes_dirty_is_told_inside_it() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let s = g.property(r, 0).unwrap();
    let t = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let a = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let m = g.binding(r, 0, move |g| g.get(a) + 10 * g.get(t)).unwrap();
    let b = g.binding(r, 0, move |g| g.get(m)).unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&order);
    g.effect_told(
        r,
        move |g| {
            g.get(b);
        },
        move || o.borrow_mut().push("told"),
    )
    .unwrap();
    let o = Rc::clone(&order);
    g.watch(r, t, move |_, _| o.borrow_mut().push("watched"))
        .unwrap();
    let hooks = Rc::clone(&g);
    g.set_note_handler(move |n| {
        if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far());
            hooks.set(t, 2);
        }
    });
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(s, 1);
        g.set(q, 1);
    });

    assert_eq!(
        *order.borrow(),
        ["told", "watched"],
        "told before the effects of the change, once"
    );
}

/// A change counted after as many others as the counter holds is a
/// change of its own all the same. The batch writes `p`, then `q`, both
/// noted; `q`'s hook opens a queue that waits on `q`'s change, so `p`'s
/// hook waits below it. 255 writes of an unrelated `u` inside the queue
/// count the changes round to where a counter of a byte stood at `q`'s,
/// and then `p` is written again: that change is made inside the queue
/// and reaches `p`, so `p`'s hook is told inside it, before `p`'s watcher
/// runs there and reads `p` with what the hook keeps of it.
#[test]
fn a_hook_a_change_inside_the_queue_reaches_is_told_inside_it_however_many_came_before() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let p = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let u = g.property(r, 0).unwrap();
    let kept = Rc::new(RefCell::new(0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, p, move |g, _| {
        let now = g.untrack(|| g.get(p));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == p {
            *k.borrow_mut() = hooks.untrack(|| hooks.get(p));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(1));
            for v in 1..=255 {
                hooks.set(u, v);
            }
            hooks.set(p, 2);
        }
    });
    g.set_noted(p).unwrap();
    g.set_noted(q).unwrap();

    g.batch(|| {
        g.set(p, 1);
        g.set(q, 1);
    });

    assert_eq!(*hooked.borrow(), [q, p], "each hook told once");
    assert_eq!(
        *seen.borrow(),
        [(2, 2)],
        "the watcher reads what the hook keeps as of the value it reads"
    );
}

/// In a handler ahead ([`Graph::watch_ahead`]), a binding a change made
/// inside an own queue reaches reads as it is, though a change the queue
/// waits on reached it first; one only those reached reads as it was.
/// The handler batches writes of `s`, `v` and then `q`; `b` reads `s` and
/// `c` reads `v`. `q`'s hook opens a queue that waits on all three, so
/// `b`'s hook waits below it, and writes `s` again: that reaches `b`, so
/// `b`'s hook is told inside the queue and reads `b` as it now is, as
/// `s`'s watcher after it does, run as a root is, outside the handler.
/// `c`, which nothing made inside reaches, the hook reads as it was.
#[test]
fn in_a_handler_ahead_a_hook_a_change_inside_the_queue_reaches_reads_what_it_made() {
    let g: Rc<Graph<i64>> = Rc::new(Graph::new());
    let r = g.root();
    let x = g.property(r, 0).unwrap();
    let s = g.property(r, 0).unwrap();
    let v = g.property(r, 0).unwrap();
    let q = g.property(r, 0).unwrap();
    let b = g.binding(r, 0, move |g| g.get(s)).unwrap();
    let c = g.binding(r, 0, move |g| g.get(v)).unwrap();
    assert_eq!((g.get(b), g.get(c)), (0, 0));
    // What `b`'s hook keeps: `b` and `c` as it reads them.
    let kept = Rc::new(RefCell::new((0, 0)));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (k, w) = (Rc::clone(&kept), Rc::clone(&seen));
    g.watch(r, s, move |g, _| {
        let now = g.untrack(|| g.get(b));
        w.borrow_mut().push((now, *k.borrow()));
    })
    .unwrap();
    let hooked = Rc::new(RefCell::new(Vec::new()));
    let (hooks, k, h) = (Rc::clone(&g), Rc::clone(&kept), Rc::clone(&hooked));
    g.set_note_handler(move |n| {
        h.borrow_mut().push(n);
        if n == b {
            *k.borrow_mut() = hooks.untrack(|| (hooks.get(b), hooks.get(c)));
        } else if n == q {
            let _own = hooks.own_queue(hooks.changes_so_far().wrapping_sub(3));
            hooks.set(s, 2);
        }
    });
    g.set_noted(b).unwrap();
    g.set_noted(q).unwrap();
    g.watch_ahead(r, x, move |g, _| {
        g.batch(|| {
            g.set(s, 1);
            g.set(v, 1);
            g.set(q, 1);
        });
    })
    .unwrap();

    g.set(x, 1);

    assert_eq!(*hooked.borrow(), [q, b], "each hook told once");
    assert_eq!(
        seen.borrow().first(),
        Some(&(2, (2, 0))),
        "the hook and the watcher read `b` as the change inside made it, \
         and the hook `c` as it was"
    );
}
