//! Numbers for docs/research and the M0 report. Run with
//! `cargo test -p ljos_js --release --test bench -- --ignored --nocapture`.

mod common;

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use common::*;
use ljos_js::{Bytecode, BytecodeOptions, JsConfig, JsHost, JsValue};

fn per_iter(label: &str, n: u32, f: impl FnOnce(u32)) {
    let t = Instant::now();
    f(n);
    let e = t.elapsed();
    println!(
        "{label}: {:.1} ns/iter ({n} iters, {e:?})",
        e.as_nanos() as f64 / f64::from(n)
    );
}

/// A module the size of a medium QML file after compilation: many small
/// binding functions.
fn big_module(fns: usize) -> String {
    let mut s = String::new();
    for i in 0..fns {
        s.push_str(&format!(
            "export function b{i}(self, parent, root) {{ const w = (self.width ?? 0) + {i}; return w > 10 ? `item ${{w}}` : [w, parent, root].length; }}\n"
        ));
    }
    s
}

#[test]
#[ignore = "benchmark"]
fn bench_calls() {
    let (host, store) = host_with_store();
    store.insert(1, 21.0, 5.0);
    let m = host
        .load_module(
            "b.js",
            "export function noop() {}\nexport function id(x) { return x; }\nexport function readw(o) { return o.width; }\nexport function read100(o) { let s = 0; for (let i = 0; i < 100; i++) s += o.width; return s; }",
        )
        .unwrap();
    let noop = m.function("noop").unwrap();
    let id = m.function("id").unwrap();
    let readw = m.function("readw").unwrap();
    let read100 = m.function("read100").unwrap();
    let arg = [JsValue::Object(1)];

    // Whole-path cost, including enter/deadline arming and result conversion.
    per_iter("JsHost::call trivial fn (full enter)", 200_000, |n| {
        for _ in 0..n {
            black_box(host.call(noop, &[]).unwrap());
        }
    });
    per_iter("JsHost::call id(number)", 200_000, |n| {
        for _ in 0..n {
            black_box(host.call(id, &[JsValue::Number(1.0)]).unwrap());
        }
    });
    per_iter(
        "JsHost::call readw(obj) (call + 1 accessor)",
        200_000,
        |n| {
            for _ in 0..n {
                black_box(host.call(readw, &arg).unwrap());
            }
        },
    );

    // Inside one enter, to separate Rust->JS call cost from enter cost.
    host.enter("bench", Default::default(), |s| {
        per_iter(
            "Scope::call trivial fn (inside one enter)",
            1_000_000,
            |n| {
                for _ in 0..n {
                    black_box(s.call(noop, &[]).unwrap());
                }
            },
        );
        Ok(())
    })
    .unwrap();

    // Accessor cost alone: 100 reads per call, minus the call itself.
    let t = Instant::now();
    let n = 20_000u32;
    for _ in 0..n {
        black_box(host.call(read100, &arg).unwrap());
    }
    let per_call = t.elapsed().as_nanos() as f64 / f64::from(n);
    println!(
        "accessor get through HostObjects: {:.1} ns/read (100 reads/call)",
        per_call / 100.0
    );
}

#[test]
#[ignore = "benchmark"]
fn bench_module_load() {
    use ljos_js::rquickjs::{Context, Module, Runtime};

    let reps = 20u32;
    let avg = |d: Duration| d.as_secs_f64() * 1e6 / f64::from(reps);
    for fns in [50usize, 500, 2000] {
        let src = big_module(fns);
        let bc = Bytecode::compile("bc.js", &src, BytecodeOptions::default()).unwrap();
        let stripped = Bytecode::compile(
            "bc.js",
            &src,
            BytecodeOptions {
                strip_source: true,
                strip_debug: true,
            },
        )
        .unwrap();

        // The raw QuickJS step: parse+compile versus read bytecode, no
        // evaluation, in a warm runtime (this is the "14x" comparison in 03).
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();
        let (mut declare, mut read) = (Duration::ZERO, Duration::ZERO);
        for i in 0..reps {
            ctx.with(|c| {
                let t = Instant::now();
                let m = Module::declare(c.clone(), format!("d{i}"), src.as_str()).unwrap();
                declare += t.elapsed();
                drop(m);
                let t = Instant::now();
                // SAFETY: bytes come from Bytecode::compile and outlive `ctx`.
                let m = unsafe { Module::load(c.clone(), bc.as_bytes()).unwrap() };
                read += t.elapsed();
                drop(m);
            });
        }

        // The full JsHost path including evaluation and handle registration.
        let (mut full_src, mut full_bc) = (Duration::ZERO, Duration::ZERO);
        for i in 0..reps {
            let h = JsHost::new(JsConfig::default()).unwrap();
            let t = Instant::now();
            h.load_module(&format!("f{i}.js"), &src).unwrap();
            full_src += t.elapsed();
            let h = JsHost::new(JsConfig::default()).unwrap();
            let t = Instant::now();
            h.load_module_bytecode(&bc).unwrap();
            full_bc += t.elapsed();
        }
        let t = Instant::now();
        for _ in 0..reps {
            black_box(Bytecode::compile("bc.js", &src, BytecodeOptions::default()).unwrap());
        }
        let compile_job = t.elapsed();
        println!(
            "{fns:>5} fns {:>7} B src | compile {:>6.0} us vs read bytecode {:>6.0} us ({:.1}x) | JsHost load: src {:>6.0} us, bytecode {:>6.0} us ({:.1}x) | bytecode {} B (stripped {} B) | Bytecode::compile job {:.0} us",
            src.len(),
            avg(declare),
            avg(read),
            declare.as_secs_f64() / read.as_secs_f64(),
            avg(full_src),
            avg(full_bc),
            full_src.as_secs_f64() / full_bc.as_secs_f64(),
            bc.as_bytes().len(),
            stripped.as_bytes().len(),
            avg(compile_job),
        );
    }
}

#[test]
#[ignore = "benchmark"]
fn bench_runtime_creation() {
    let n = 2000u32;
    let t = Instant::now();
    for _ in 0..n {
        black_box(JsHost::new(JsConfig::default()).unwrap());
    }
    println!(
        "JsHost::new + drop (runtime + full context): {:.1} us",
        t.elapsed().as_secs_f64() * 1e6 / f64::from(n)
    );
    let host = JsHost::new(JsConfig::default()).unwrap();
    let s = host.memory_stats().unwrap();
    println!(
        "fresh JsHost heap: {:.1} KB in {} allocations ({} objects)",
        s.malloc_size as f64 / 1024.0,
        s.malloc_count,
        s.object_count
    );
    // Reference point for the 28 KB runtime / 54 KB context figures in 03.
    let (host, _) = host_with_store();
    let s = host.memory_stats().unwrap();
    println!(
        "JsHost + one registered type: {:.1} KB",
        s.malloc_size as f64 / 1024.0
    );
}
