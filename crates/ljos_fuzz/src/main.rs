//! `ljos-fuzz`: compares Ljós with Qt on random programs (see the crate's
//! documentation) and prints the buckets it finds.

use std::{path::PathBuf, process::ExitCode, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ljos_fuzz::{
    area::Area,
    bindings::Bindings,
    bucket::Known,
    connections::Connections,
    creation::Creation,
    geometry::Geometry,
    models::Models,
    rng::Rng,
    run::{Config, Oracle, Report, run},
    shrink::Budget,
};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AreaName {
    Bindings,
    Geometry,
    Connections,
    Creation,
    Models,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OracleName {
    Qt,
    Planted,
}

/// Differential testing of Ljós against Qt (qml6).
#[derive(Debug, Parser)]
#[command(name = "ljos-fuzz")]
struct Args {
    /// What to generate.
    #[arg(value_enum, default_value = "bindings")]
    area: AreaName,
    #[arg(long, default_value_t = 500)]
    cases: usize,
    /// The run's seed; a fresh one when left out. Case `n` of a seed is the
    /// same program on every machine.
    #[arg(long)]
    seed: Option<u64>,
    /// The directory of known buckets, one `<area>.txt` per area; every
    /// area knows `bindings.txt` too.
    #[arg(long, default_value = "crates/ljos_fuzz/known")]
    known: PathBuf,
    /// Where repros are written, one directory per bucket.
    #[arg(long, default_value = "target/fuzz/out")]
    out: PathBuf,
    /// Where Qt's batches are written.
    #[arg(long, default_value = "target/fuzz/work")]
    work: PathBuf,
    #[arg(long, value_enum, default_value = "qt")]
    oracle: OracleName,
    #[arg(long, default_value = "qml6")]
    qml: String,
    /// Threads per side; all cores when left out.
    #[arg(long)]
    jobs: Option<usize>,
    /// Report differences as found, without shrinking them.
    #[arg(long)]
    no_shrink: bool,
    /// Print case `N` of the seed as both sides run it, and nothing else.
    #[arg(long, value_name = "N")]
    show: Option<u64>,
    /// Seconds spent shrinking one bucket at most.
    #[arg(long, default_value_t = 90)]
    shrink_secs: u64,
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let seed = args.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_secs())
    });
    let jobs = args
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, usize::from));
    match args.area {
        AreaName::Bindings => fuzz(&Bindings, args, seed, jobs),
        AreaName::Geometry => fuzz(&Geometry, args, seed, jobs),
        AreaName::Connections => fuzz(&Connections, args, seed, jobs),
        AreaName::Creation => fuzz(&Creation, args, seed, jobs),
        AreaName::Models => fuzz(&Models, args, seed, jobs),
    }
}

fn fuzz<A: Area>(area: &A, args: Args, seed: u64, jobs: usize) -> Result<ExitCode> {
    if let Some(n) = args.show {
        let case = area.render(&area.generate(&mut Rng::for_case(seed, n)));
        print!("{}", case.qml);
        return Ok(ExitCode::SUCCESS);
    }
    let mut known = Known::read(&args.known.join(format!("{}.txt", area.name())))?;
    // Every area's expressions are bindings', so what is known there is
    // known here too, unless the area's own list says otherwise.
    if area.name() != "bindings" {
        for (sig, why) in Known::read(&args.known.join("bindings.txt"))?.0 {
            known
                .0
                .entry(sig)
                .or_insert_with(|| format!("as in bindings: {why}"));
        }
    }
    let work = std::path::absolute(&args.work).context("the work directory")?;
    // Old repros would read as this run's.
    if args.out.exists() {
        std::fs::remove_dir_all(&args.out)
            .with_context(|| format!("clearing {}", args.out.display()))?;
    }
    let config = Config {
        cases: args.cases,
        seed,
        jobs,
        oracle: match args.oracle {
            OracleName::Qt => Oracle::Qt {
                qml: args.qml,
                work,
            },
            OracleName::Planted => Oracle::Planted,
        },
        known,
        out: args.out,
        shrink: !args.no_shrink,
        budget: Budget {
            time: Duration::from_secs(args.shrink_secs),
            ..Budget::default()
        },
        max_buckets: 60,
        per_group: 3,
    };
    println!(
        "ljos-fuzz {}: seed {seed}, {} cases",
        area.name(),
        config.cases
    );
    let report = run(area, &config)?;
    print(&report);
    Ok(if report.new_buckets() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn print(r: &Report) {
    let rate = |d: Duration| r.cases as f64 / d.as_secs_f64().max(1e-9);
    let pct = r.rejected.len() as f64 * 100.0 / r.cases.max(1) as f64;
    println!(
        "  qt:   {:.1} s, {:.0} cases/s; {} rejected ({pct:.1} %), {} crashed",
        r.qt.as_secs_f64(),
        rate(r.qt),
        r.rejected.len(),
        r.qt_crashed.len()
    );
    println!(
        "  ljos: {:.1} s, {:.0} cases/s",
        r.ljos.as_secs_f64(),
        rate(r.ljos)
    );
    let n = r.buckets.len();
    println!(
        "  {} agree, {} differ, {n} bucket{} ({} new)",
        r.agree,
        r.differ,
        if n == 1 { "" } else { "s" },
        r.new_buckets()
    );
    // By what Qt said, without where: a generator fault shows as one line.
    let mut why: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (n, e) in r.rejected.iter().chain(&r.qt_crashed) {
        let line = e.lines().next().unwrap_or("");
        let text = line.rsplit_once(".qml:").map_or(line, |(_, rest)| rest);
        let text = text
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == ':')
            .trim();
        why.entry(text.to_owned()).or_insert((*n, 0)).1 += 1;
    }
    for (text, (first, count)) in &why {
        println!("  qt did not run {count} (first: case {first}): {text}");
    }
    for b in &r.buckets {
        let state = if b.known.is_some() { "known" } else { "NEW  " };
        println!(
            "  {state} {}  ({}; {} case{})  {}",
            b.signature,
            b.summary,
            b.cases,
            if b.cases == 1 { "" } else { "s" },
            b.repro.display()
        );
        if let Some(why) = &b.known {
            println!("        {why}");
        }
    }
}
