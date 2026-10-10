//! Propagation cost on 10k nodes. Run with
//! `cargo test --release -p ljos_reactive --test bench -- --ignored --nocapture`.

use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;
use std::time::Instant;

use ljos_reactive::{Graph, NodeId};

const N: usize = 10_000;
const ROUNDS: u32 = 200;

fn report(name: &str, rounds: u32, start: Instant, evals: u64) {
    let per = start.elapsed() / rounds;
    println!(
        "{name:<34} {per:>10.2?} per write  ({:.1} ns/node, {} evals/write)",
        per.as_nanos() as f64 / N as f64,
        evals / u64::from(rounds)
    );
}

/// A chain a -> b1 -> ... -> b10000 with an effect at the end.
#[test]
#[ignore]
fn chain_10k() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let src = g.property(r, 0).unwrap();
    let mut prev = src;
    for _ in 0..N {
        let p = prev;
        prev = g.binding(r, 0, move |g| g.get(p) + 1).unwrap();
        // Evaluate as we go so the first pull is not one deep recursion.
        g.get(prev);
    }
    let last = prev;
    let sink = Rc::new(Cell::new(0));
    let s = sink.clone();
    g.effect(r, move |g| s.set(g.get(last))).unwrap();
    let before = g.stats().evaluations;
    let start = Instant::now();
    for i in 1..=ROUNDS {
        g.set(src, i64::from(i));
    }
    report(
        "chain, effect at the end",
        ROUNDS,
        start,
        g.stats().evaluations - before,
    );
    assert_eq!(sink.get(), i64::from(ROUNDS) + N as i64);
}

/// One source read by 10k bindings, each watched (the I18n revision shape).
#[test]
#[ignore]
fn fan_out_10k() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let src = g.property(r, 0).unwrap();
    let hits = Rc::new(Cell::new(0u64));
    for i in 0..N as i64 {
        let b = g.binding(r, 0, move |g| g.get(src) + i).unwrap();
        let h = hits.clone();
        g.watch(r, b, move |_, _| h.set(h.get() + 1)).unwrap();
    }
    let before = g.stats().evaluations;
    let start = Instant::now();
    for i in 1..=ROUNDS {
        g.set(src, i64::from(i));
    }
    report(
        "fan-out, all watched",
        ROUNDS,
        start,
        g.stats().evaluations - before,
    );
    assert_eq!(hits.get(), u64::from(ROUNDS) * N as u64);
}

/// The same fan-out, unobserved: lazy bindings cost only marking.
#[test]
#[ignore]
fn fan_out_10k_lazy_unread() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let src = g.property(r, 0).unwrap();
    let nodes: Vec<NodeId> = (0..N as i64)
        .map(|i| g.binding(r, 0, move |g| g.get(src) + i).unwrap())
        .collect();
    for n in &nodes {
        g.get(*n);
    }
    let start = Instant::now();
    // Each write re-dirties nothing new after the first: marking stops at
    // already-dirty nodes. Read one node per round to keep it honest.
    for i in 1..=ROUNDS {
        g.set(src, i64::from(i));
        black_box(g.get(nodes[i as usize % N]));
    }
    report("fan-out, lazy, one read", ROUNDS, start, 0);
}

/// Steady-state reads of clean nodes: no allocation, no evaluation.
#[test]
#[ignore]
fn clean_reads_10k() {
    let g: Graph<i64> = Graph::new();
    let r = g.root();
    let nodes: Vec<NodeId> = (0..N as i64).map(|i| g.property(r, i).unwrap()).collect();
    let start = Instant::now();
    let mut sum = 0;
    for _ in 0..ROUNDS {
        for n in &nodes {
            sum += g.get(*n);
        }
    }
    report("clean reads (10k per round)", ROUNDS, start, 0);
    black_box(sum);
}
