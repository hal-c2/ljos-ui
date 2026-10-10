//! Rough timings, ignored by default:
//!
//! `cargo test --release -p ljos_engine --test bench -- --ignored --nocapture`

mod common;

use std::hint::black_box;
use std::time::Instant;

use common::{Item, Rectangle, engine};
use ljos_engine::Value;

fn report(what: &str, n: u32, f: impl FnOnce()) {
    let t = Instant::now();
    f();
    let ns = t.elapsed().as_nanos() as f64 / f64::from(n);
    println!("{what:<44} {ns:>9.1} ns/op");
}

#[test]
#[ignore = "benchmark"]
fn bench_property_read_clean() {
    let e = engine();
    let r = e.create_by_name("Test", "Rectangle", None).unwrap();
    e.set(r, Item::WIDTH, 3.0).unwrap();
    let n = 1_000_000;
    report("read, typed handle, no node", n, || {
        for _ in 0..n {
            black_box(e.get(black_box(r), Item::WIDTH).unwrap());
        }
    });
    report("read, by index, no node", n, || {
        for _ in 0..n {
            black_box(e.read(black_box(r), Item::WIDTH.index));
        }
    });
    report("read, by name, no node", n, || {
        for _ in 0..n {
            black_box(e.read_by_name(black_box(r), "width"));
        }
    });
    // With a node (something observes it), still clean.
    let o = e.create_by_name("Test", "Rectangle", None).unwrap();
    e.bind(o, Item::WIDTH.index, move |e| e.read(r, Item::WIDTH.index))
        .unwrap();
    let _ = e.read(o, Item::WIDTH.index);
    report("read, by index, clean node", n, || {
        for _ in 0..n {
            black_box(e.read(black_box(r), Item::WIDTH.index));
        }
    });
    report("read, clean binding", n, || {
        for _ in 0..n {
            black_box(e.read(black_box(o), Item::WIDTH.index));
        }
    });
}

#[test]
#[ignore = "benchmark"]
fn bench_write_notify_one_observer() {
    let e = engine();
    let r = e.create_by_name("Test", "Rectangle", None).unwrap();
    let n = 200_000;
    report("write, no observer", n, || {
        for i in 0..n {
            e.set(r, Item::WIDTH, f64::from(i)).unwrap();
        }
    });
    let count = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let c = count.clone();
    e.connect_by_name(r, "radiusChanged", move |_, _| c.set(c.get() + 1))
        .unwrap();
    report("write + notify, 1 handler", n, || {
        for i in 0..n {
            e.set(r, Rectangle::RADIUS, f64::from(i)).unwrap();
        }
    });
    assert_eq!(count.get(), n - 1);
    let o = e.create_by_name("Test", "Rectangle", None).unwrap();
    e.bind(o, Item::HEIGHT.index, move |e| e.read(r, Item::WIDTH.index))
        .unwrap();
    report("write + pull, 1 binding", n, || {
        for i in 0..n {
            e.write(
                r,
                Item::WIDTH.index,
                Value::Real(f64::from(i) + 0.5),
                ljos_engine::WriteKind::Imperative,
            )
            .unwrap();
            black_box(e.read(o, Item::HEIGHT.index));
        }
    });
}

#[test]
#[ignore = "benchmark"]
fn bench_create_destroy_10k() {
    let e = engine();
    let meta = e.lookup_type("Test", "Rectangle").unwrap();
    let n = 10_000;
    for round in 0..3 {
        let mut ids = Vec::with_capacity(n as usize);
        report(&format!("create Rectangle (round {round})"), n, || {
            for _ in 0..n {
                ids.push(e.create(meta.clone(), None).unwrap());
            }
        });
        report(&format!("destroy Rectangle (round {round})"), n, || {
            for id in &ids {
                e.destroy_now(*id);
            }
        });
    }
    // With nodes: each object observed once. Two sizes, to show the cost
    // per object does not grow with the count.
    for n in [1_000u32, 10_000] {
        let mut ids = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let id = e.create(meta.clone(), None).unwrap();
            e.connect_by_name(id, "widthChanged", |_, _| {}).unwrap();
            ids.push(id);
        }
        report(&format!("destroy {n} with a node + connection"), n, || {
            for id in &ids {
                e.destroy_now(*id);
            }
        });
    }
    assert_eq!(e.object_count(), 0);
}
