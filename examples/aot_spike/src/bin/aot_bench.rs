//! The measurements of research note 13, one kind per run so that each
//! mode's memory is its own process's:
//!
//! ```text
//! aot_bench cases <mode>      per case: median and p99 ns of a trigger
//! aot_bench boundary <mode>   ns per tracked read, write and emit
//! aot_bench rss <mode>        VmRSS and VmHWM after the cases ran
//! aot_bench pauses            back end A: slowest call, and a full collection
//! ```
//!
//! Each sample is one trigger, timed alone; its inputs are set before the
//! clock starts. Nothing is repeated to smooth a number.

use std::time::Instant;

use anyhow::{Context, Result, bail};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_example_aot_spike::{BENCH, CASES, Mode, compile, drive, engine, load, prepare, trigger};

const SAMPLES: usize = 2000;
const WARM: usize = 200;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = |i: usize| -> Result<Mode> {
        let s = args.get(i).context("a mode")?;
        Mode::parse(s).with_context(|| format!("no mode {s}"))
    };
    match args.first().map(String::as_str) {
        Some("cases") => cases(mode(1)?),
        Some("boundary") => boundary(mode(1)?),
        Some("rss") => rss(mode(1)?),
        #[cfg(feature = "scriptc")]
        Some("pauses") => pauses(),
        _ => bail!("usage: aot_bench cases|boundary|rss <mode> | pauses"),
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn cases(mode: Mode) -> Result<()> {
    println!("case\tmedian_ns\tp99_ns\ttier1_runs_per_trigger");
    for case in CASES {
        let run = load(mode, false);
        for k in 0..WARM {
            prepare(&run, case, k);
            trigger(&run, case, k);
        }
        let mut ns = Vec::with_capacity(SAMPLES);
        let mut tier1 = 0;
        for k in WARM..WARM + SAMPLES {
            prepare(&run, case, k);
            let before = run.engine.tier1_stats().runs;
            let t = Instant::now();
            trigger(&run, case, k);
            ns.push(t.elapsed().as_nanos() as u64);
            tier1 += run.engine.tier1_stats().runs - before;
        }
        ns.sort_unstable();
        println!(
            "{case}\t{}\t{}\t{:.2}",
            percentile(&ns, 0.5),
            percentile(&ns, 0.99),
            tier1 as f64 / SAMPLES as f64
        );
    }
    Ok(())
}

/// Median ns of `f` over the samples, `prep` run untimed before each.
fn median(mut prep: impl FnMut(usize), mut f: impl FnMut(usize)) -> f64 {
    for k in 0..WARM {
        prep(k);
        f(k);
    }
    let mut ns: Vec<u64> = (0..SAMPLES)
        .map(|k| {
            prep(k);
            let t = Instant::now();
            f(k);
            t.elapsed().as_nanos() as u64
        })
        .collect();
    ns.sort_unstable();
    percentile(&ns, 0.5) as f64
}

/// Per one more operation: (cost of 33 − cost of 1) / 32.
fn per(one: f64, many: f64) -> f64 {
    (many - one) / 32.0
}

fn boundary(mode: Mode) -> Result<()> {
    let (read, write, emit, entry) = match mode {
        Mode::Tier1 | Mode::QuickJs => dynamic_boundary(mode)?,
        Mode::B => b_boundary()?,
        #[cfg(feature = "scriptc")]
        Mode::A => a_boundary()?,
    };
    println!("read_ns\twrite_ns\temit_ns\tentry_ns");
    println!("{read:.1}\t{write:.1}\t{emit:.1}\t{entry:.1}");
    Ok(())
}

fn dynamic_boundary(mode: Mode) -> Result<(f64, f64, f64, f64)> {
    let e = engine();
    e.set_tier1(mode == Mode::Tier1);
    e.set_component_source(compile(&[("Bench", BENCH)]));
    let o = e.create_from_url("Bench", None, Vec::new())?;
    e.connect_by_name(o, "ping", |_, _| {})?;
    let p = |n: &str| e.property_index(o, n);
    let (k, r1, r33) = (p("k")?, p("r1")?, p("r33")?);
    let set = |prop, v: usize| {
        e.write(
            o,
            prop,
            Value::Real(v as f64),
            ljos_engine::WriteKind::Imperative,
        )
        .expect("write");
    };
    let before = e.tier1_stats().runs;
    let one = median(|i| set(k, i), |_| drop(std::hint::black_box(e.read(o, r1))));
    let many = median(
        |i| set(k, i),
        |_| drop(std::hint::black_box(e.read(o, r33))),
    );
    let ran = e.tier1_stats().runs - before;
    let read = per(one, many);
    // Writes and emits from a handler: the handler that does one against
    // the handler that does none, each started by the same write.
    let (kn, kw, ke) = (p("kn")?, p("kw")?, p("ke")?);
    let none = median(|_| {}, |i| set(kn, i));
    let before = e.tier1_stats().runs;
    let write = median(|_| {}, |i| set(kw, i)) - none;
    let wran = e.tier1_stats().runs - before;
    let before = e.tier1_stats().runs;
    let emit = median(|_| {}, |i| set(ke, i)) - none;
    let eran = e.tier1_stats().runs - before;
    eprintln!(
        "{}: r1 {one} ns, r33 {many} ns, kn {none} ns; tier 1 ran {ran} of {} reads, {wran} and {eran} of {} handlers",
        mode.name(),
        2 * (SAMPLES + WARM),
        SAMPLES + WARM
    );
    // Entering a binding, less the read it makes.
    Ok((read, write, emit, one - read))
}

fn b_boundary() -> Result<(f64, f64, f64, f64)> {
    let run = load(Mode::B, false);
    let e = &run.engine;
    let root = run.root;
    let done = e.property_index(root, "done")?;
    let cursor = e.property_index(root, "cursor")?;
    let dispatched = e.signal_index(root, "dispatched")?;
    let x = Value::String("x".into());
    // What B generates for `n` reads in a binding: one read_number each.
    let bind = |n: u32| -> Result<ObjectId> {
        let o = run.find("bar");
        let w = e.property_index(o, "width")?;
        e.bind(o, w, move |e: &Engine| {
            let mut s = 0.0;
            for _ in 0..n {
                s += e.read_number(root, done);
            }
            Value::Real(s)
        })?;
        Ok(o)
    };
    let set_done = |i: usize| {
        e.write(
            root,
            done,
            Value::Real(i as f64),
            ljos_engine::WriteKind::Imperative,
        )
        .expect("w")
    };
    let o = bind(1)?;
    let w = e.property_index(o, "width")?;
    let one = median(set_done, |_| drop(std::hint::black_box(e.read(o, w))));
    bind(33)?;
    let many = median(set_done, |_| drop(std::hint::black_box(e.read(o, w))));
    let writes = |n: usize| {
        median(
            |_| {},
            |i| {
                for j in 0..n {
                    ljos_example_aot_spike::rt::write(
                        e,
                        root,
                        cursor,
                        Value::Real((i * 33 + j) as f64),
                    );
                }
            },
        )
    };
    let emits = |n: usize| {
        median(
            |_| {},
            |_| {
                for _ in 0..n {
                    ljos_example_aot_spike::rt::emit(e, root, dispatched, std::slice::from_ref(&x));
                }
            },
        )
    };
    let write = per(writes(1), writes(33));
    let emit = per(emits(1), emits(33));
    eprintln!("b: 1 read {one} ns, 33 reads {many} ns");
    let read = per(one, many);
    Ok((read, write, emit, one - read))
}

#[cfg(feature = "scriptc")]
fn a_boundary() -> Result<(f64, f64, f64, f64)> {
    use ljos_example_aot_spike::a::{self, Probe};
    let run = load(Mode::A, false);
    let e = &run.engine;
    let root = run.root;
    let l = a::link(e, root)?;
    let l = std::rc::Rc::new(l);
    let done = e.property_index(root, "done")?;
    let o = l.bar;
    let w = e.property_index(o, "width")?;
    let bind = |p: Probe, n: f64| {
        let l = l.clone();
        e.bind(o, w, move |e: &Engine| Value::Real(a::probe(e, &l, &p, n)))
            .expect("bind");
    };
    let set_done = |i: usize| {
        e.write(
            root,
            done,
            Value::Real(i as f64),
            ljos_engine::WriteKind::Imperative,
        )
        .expect("w")
    };
    let read = |n: f64| {
        bind(Probe::Read, n);
        median(set_done, |_| drop(std::hint::black_box(e.read(o, w))))
    };
    let (one, many) = (read(1.0), read(33.0));
    let calls = |p: Probe, n: f64| {
        median(
            |_| {},
            |_| {
                std::hint::black_box(a::probe(e, &l, &p, n));
            },
        )
    };
    let write = per(calls(Probe::Write, 1.0), calls(Probe::Write, 33.0));
    let emit = per(calls(Probe::Emit, 1.0), calls(Probe::Emit, 33.0));
    let empty = calls(Probe::Nothing, 0.0);
    eprintln!("a: 1 read {one} ns, 33 reads {many} ns, a call that does nothing {empty} ns");
    let read = per(one, many);
    Ok((read, write, emit, one - read))
}

fn status(field: &str) -> Result<u64> {
    let s = std::fs::read_to_string("/proc/self/status")?;
    let line = s
        .lines()
        .find(|l| l.starts_with(field))
        .context("no such field")?;
    Ok(line
        .split_whitespace()
        .nth(1)
        .context("a number")?
        .parse()?)
}

fn rss(mode: Mode) -> Result<()> {
    let before = status("VmRSS:")?;
    let run = load(mode, false);
    let loaded = status("VmRSS:")?;
    drive(&run, 0..SAMPLES);
    println!("mode\tstart_kb\tloaded_kb\tafter_kb\thwm_kb");
    println!(
        "{}\t{before}\t{loaded}\t{}\t{}",
        mode.name(),
        status("VmRSS:")?,
        status("VmHWM:")?
    );
    Ok(())
}

/// Back end A's collector: the library frees by reference count and
/// collects cycles in a full pass. The slowest single trigger over the
/// cases shows whether a pass ever lands inside one; a pass forced after
/// the run shows what one costs.
#[cfg(feature = "scriptc")]
fn pauses() -> Result<()> {
    let run = load(Mode::A, false);
    let mut worst = (0u64, "");
    let mut all = Vec::new();
    for k in 0..SAMPLES {
        for case in CASES {
            prepare(&run, case, k);
            let t = Instant::now();
            trigger(&run, case, k);
            let ns = t.elapsed().as_nanos() as u64;
            all.push(ns);
            if ns > worst.0 {
                worst = (ns, case);
            }
        }
    }
    all.sort_unstable();
    let first = ljos_example_aot_spike::a::collect();
    let second = ljos_example_aot_spike::a::collect();
    println!("triggers\tp999_ns\tworst_ns\tworst_case\tcollect_ns\tcollect_again_ns\tframe_ns");
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}\t4000000",
        all.len(),
        percentile(&all, 0.999),
        worst.0,
        worst.1,
        first.as_nanos(),
        second.as_nanos()
    );
    Ok(())
}
