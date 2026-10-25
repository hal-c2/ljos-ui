//! How long a Lottie file takes to parse and to turn each frame into
//! paths, and how many paths a frame has:
//! `cargo run --release -p ljos_lottie --example frame_time -- file.json [rounds]`.
//!
//! Drawing the paths is the backend's work and is not timed here.

use std::time::Instant;

use ljos::item::kurbo::Affine;
use ljos_lottie::{model, render};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: frame_time file.json [rounds]");
        std::process::exit(2);
    };
    let rounds: u32 = args.next().and_then(|r| r.parse().ok()).unwrap_or(10);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });
    let started = Instant::now();
    let comp = model::parse(&bytes).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });
    let parse = started.elapsed();
    let (first, last) = (comp.in_point as i64, comp.out_point as i64);
    let mut ops = 0usize;
    let mut frames = 0u32;
    let mut worst = std::time::Duration::ZERO;
    let started = Instant::now();
    for _ in 0..rounds {
        for f in first..last {
            let one = Instant::now();
            ops += render::render(&comp, f as f64, Affine::IDENTITY).len();
            worst = worst.max(one.elapsed());
            frames += 1;
        }
    }
    let total = started.elapsed();
    println!(
        "{path}: parse {:.2} ms, {frames} frames, {:.1} µs a frame (worst {:.1} µs), {:.1} paths a frame",
        parse.as_secs_f64() * 1e3,
        total.as_secs_f64() * 1e6 / f64::from(frames.max(1)),
        worst.as_secs_f64() * 1e6,
        ops as f64 / f64::from(frames.max(1)),
    );
}
