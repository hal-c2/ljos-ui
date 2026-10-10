//! Cost of disposing one scope as the graph grows. The cost must be flat:
//! child-scope removal and observer-list removal are both O(1).
//!
//! Run with
//! `cargo test --release -p ljos_reactive --test dispose_bench -- --ignored --nocapture`.

use std::hint::black_box;
use std::time::Instant;

use ljos_reactive::{Graph, ScopeId};

/// Disposals timed per size. Fixed, so only the graph size varies.
const SAMPLE: usize = 2_000;

/// Deterministic shuffle: the order must not favour either end of a list.
fn shuffled(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    for i in (1..n).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
    v
}

/// `n` sibling scopes ("objects"), each with a property and a binding that
/// reads one shared source, so the source has `n` observers.
fn build(n: usize) -> (Graph<i64>, Vec<ScopeId>) {
    let g: Graph<i64> = Graph::new();
    let root = g.root();
    let shared = g.property(root, 1).unwrap();
    let mut scopes = Vec::with_capacity(n);
    for i in 0..n {
        let s = g.create_scope(root).unwrap();
        let own = g.property(s, i as i64).unwrap();
        let b = g
            .binding(s, 0, move |g| g.get(shared) + g.get(own))
            .unwrap();
        g.get(b);
        scopes.push(s);
    }
    (g, scopes)
}

fn per_dispose(n: usize) -> f64 {
    let (g, scopes) = build(n);
    let order = shuffled(n);
    let k = SAMPLE.min(n);
    let start = Instant::now();
    for &i in &order[..k] {
        g.dispose_scope(black_box(scopes[i])).unwrap();
    }
    start.elapsed().as_nanos() as f64 / k as f64
}

/// Create and dispose one more object in a graph of `n`. The new object's
/// memory is cache-hot, so this isolates the algorithmic cost from cache
/// misses on the long lists.
fn per_churn(n: usize) -> f64 {
    let (g, _scopes) = build(n);
    let root = g.root();
    let shared = g.property(root, 1).unwrap();
    let mut spent = std::time::Duration::ZERO;
    for i in 0..SAMPLE {
        let s = g.create_scope(root).unwrap();
        let own = g.property(s, i as i64).unwrap();
        let b = g
            .binding(s, 0, move |g| g.get(shared) + g.get(own))
            .unwrap();
        g.get(b);
        let start = Instant::now();
        g.dispose_scope(black_box(s)).unwrap();
        spent += start.elapsed();
    }
    spent.as_nanos() as f64 / SAMPLE as f64
}

fn best_of_three(f: impl Fn() -> f64) -> f64 {
    // The minimum filters scheduler noise.
    (0..3).map(|_| f()).fold(f64::MAX, f64::min)
}

#[test]
#[ignore]
fn dispose_cost_is_flat() {
    println!(
        "{:>8}  {:>20}  {:>20}",
        "scopes", "churn ns/dispose", "random ns/dispose"
    );
    let mut churn = Vec::new();
    for n in [1_000, 10_000, 100_000] {
        let hot = best_of_three(|| per_churn(n));
        let random = best_of_three(|| per_dispose(n));
        println!("{n:>8}  {hot:>20.0}  {random:>20.0}");
        churn.push(hot);
    }
    // Generous bound; a linear search is two orders of magnitude over it.
    // The random column also pays DRAM latency on 100k-node lists, so it is
    // reported but not asserted.
    assert!(
        churn[2] < churn[0] * 4.0 + 1_000.0,
        "dispose cost grows with graph size: {churn:?}"
    );
}
