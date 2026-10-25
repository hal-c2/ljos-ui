//! One fuzzing run: generate, run both sides, compare, group the
//! differences, shrink one case per group and write its repro.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{
    Error,
    area::{Area, coarse, signature},
    bucket::Known,
    compare::{Diff, Verdict, compare},
    outcome::{Outcome, Rendered},
    report,
    rng::Rng,
    shrink::{Budget, shrink},
    side::{LjosSide, QtSide, Side},
};

/// What the Ljós side is compared with.
#[derive(Clone, Debug)]
pub enum Oracle {
    /// `qml6` at this path, writing its batches under `work`.
    Qt { qml: String, work: PathBuf },
    /// Ljós again, with every `int` it observes one higher: a difference
    /// planted on purpose, to test the fuzzer itself without Qt.
    Planted,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub cases: usize,
    pub seed: u64,
    pub jobs: usize,
    pub oracle: Oracle,
    pub known: Known,
    pub out: PathBuf,
    pub shrink: bool,
    pub budget: Budget,
    /// Cases shrunk at most, over all groups.
    pub max_buckets: usize,
    /// Cases shrunk at most in one group of differences of one kind on
    /// one value type.
    pub per_group: usize,
}

/// A group of differences with one signature.
#[derive(Clone, Debug)]
pub struct Bucket {
    pub signature: String,
    pub known: Option<String>,
    /// Generated cases that showed it.
    pub cases: usize,
    /// The size of its repro, in the area's words.
    pub summary: String,
    pub repro: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub area: &'static str,
    pub seed: u64,
    pub cases: usize,
    pub qt: Duration,
    pub ljos: Duration,
    /// Cases Qt did not load: the generator's faults, not Ljós's.
    pub rejected: Vec<(usize, String)>,
    pub qt_crashed: Vec<(usize, String)>,
    pub agree: usize,
    pub differ: usize,
    pub buckets: Vec<Bucket>,
}

impl Report {
    #[must_use]
    pub fn new_buckets(&self) -> usize {
        self.buckets.iter().filter(|b| b.known.is_none()).count()
    }
}

/// Ljós with a difference planted: see [`Oracle::Planted`].
struct Planted(LjosSide);

impl Side for Planted {
    fn name(&self) -> &'static str {
        "planted"
    }

    fn run(&self, cases: &[Rendered]) -> Vec<Outcome> {
        let mut out = self.0.run(cases);
        for (o, case) in out.iter_mut().zip(cases) {
            let Outcome::Ran(phases) = o else { continue };
            for phase in phases {
                for (key, v) in &mut phase.obs {
                    if case.types.get(key).is_some_and(|t| t == "int")
                        && let Some(n) = v.as_f64()
                    {
                        *v = Value::from(n + 1.0);
                    }
                }
            }
        }
        out
    }
}

/// Both sides over the same cases at once, and how long each took.
fn judge(qt: &dyn Side, ljos: &dyn Side, cases: &[Rendered]) -> (Vec<Verdict>, Duration, Duration) {
    let timed = |side: &dyn Side| {
        let start = Instant::now();
        let out = side.run(cases);
        (out, start.elapsed())
    };
    let ((q, qt_time), (l, ljos_time)) = thread::scope(|s| {
        let q = s.spawn(|| timed(qt));
        let l = timed(ljos);
        (q.join().expect("the Qt side does not panic"), l)
    });
    let verdicts = q
        .iter()
        .zip(&l)
        .zip(cases)
        .map(|((q, l), c)| compare(q, l, &c.unobserved))
        .collect();
    (verdicts, qt_time, ljos_time)
}

/// Runs `config.cases` cases of `area`.
///
/// # Errors
/// When a repro cannot be written.
pub fn run<A: Area>(area: &A, config: &Config) -> Result<Report, Error> {
    let ljos = LjosSide { jobs: config.jobs };
    let qt: Box<dyn Side> = match &config.oracle {
        Oracle::Qt { qml, work } => Box::new(QtSide::new(qml.clone(), work.clone(), config.jobs)),
        Oracle::Planted => Box::new(Planted(LjosSide { jobs: config.jobs })),
    };
    let cases: Vec<A::Case> = (0..config.cases)
        .map(|n| area.generate(&mut Rng::for_case(config.seed, n as u64)))
        .collect();
    let rendered: Vec<Rendered> = cases.iter().map(|c| area.render(c)).collect();
    let (verdicts, qt_time, ljos_time) = judge(qt.as_ref(), &ljos, &rendered);

    let mut report = Report {
        area: area.name(),
        seed: config.seed,
        cases: config.cases,
        qt: qt_time,
        ljos: ljos_time,
        rejected: Vec::new(),
        qt_crashed: Vec::new(),
        agree: 0,
        differ: 0,
        buckets: Vec::new(),
    };
    // Grouped before shrinking by what `coarse` keeps: where an unshrunk
    // case shows a difference is mostly noise. A few representatives of
    // each group are shrunk, the smallest first and each with another
    // signature, so that two bugs of one class still come out as two
    // buckets.
    let mut groups: BTreeMap<String, Vec<(String, usize, Diff)>> = BTreeMap::new();
    for (i, v) in verdicts.into_iter().enumerate() {
        match v {
            Verdict::Agree => report.agree += 1,
            Verdict::QtRejected(e) => report.rejected.push((i, e)),
            Verdict::QtCrashed(e) => report.qt_crashed.push((i, e)),
            Verdict::Differ(diffs) => {
                report.differ += 1;
                for d in diffs {
                    let group = groups.entry(coarse(area, &cases[i], &d)).or_default();
                    group.push((signature(area, &cases[i], &d), i, d));
                }
            }
        }
    }
    // The smallest groups go first: a big group is the likeliest to be one
    // known bug, and the shrink budget is shared.
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by_key(Vec::len);
    let mut tasks: Vec<(usize, Diff, usize)> = Vec::new();
    for group in &mut groups {
        group.sort_by_key(|(_, i, _)| rendered[*i].qml.len());
        let mut picked: Vec<(String, usize, Diff)> = Vec::new();
        for (sig, i, d) in group.iter() {
            if picked.len() < config.per_group && picked.iter().all(|(s, _, _)| s != sig) {
                picked.push((sig.clone(), *i, d.clone()));
            }
        }
        // Each case is counted once: on the first representative with one
        // of the case's signatures, or on the first of all if it has none.
        let mut sigs: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
        for (sig, i, _) in group.iter() {
            sigs.entry(*i).or_default().insert(sig.as_str());
        }
        let mut counts = vec![0; picked.len()];
        for of_case in sigs.values() {
            let k = picked
                .iter()
                .position(|(s, _, _)| of_case.contains(s.as_str()));
            counts[k.unwrap_or(0)] += 1;
        }
        for ((_, i, d), n) in picked.into_iter().zip(counts) {
            tasks.push((i, d, n));
        }
    }
    tasks.truncate(config.max_buckets);
    let found = spread_shrinks(area, qt.as_ref(), &ljos, config, &cases, &tasks);

    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    for ((case, diff), (_, _, count)) in found.into_iter().zip(&tasks) {
        let sig = signature(area, &case, &diff);
        if let Some(b) = buckets.get_mut(&sig) {
            b.cases += count;
            continue;
        }
        let repro = report::write(area, &config.out, &sig, &case, &diff)?;
        buckets.insert(
            sig.clone(),
            Bucket {
                known: config.known.reason(&sig).map(str::to_owned),
                signature: sig,
                cases: *count,
                summary: area.summary(&case),
                repro,
            },
        );
    }
    report.buckets = buckets.into_values().collect();
    Ok(report)
}

/// Shrinks every task's case, a few at once: each shrink waits on one
/// `qml6` at a time, which leaves the machine's other cores idle.
fn spread_shrinks<A: Area>(
    area: &A,
    qt: &dyn Side,
    ljos: &dyn Side,
    config: &Config,
    cases: &[A::Case],
    tasks: &[(usize, Diff, usize)],
) -> Vec<(A::Case, Diff)> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(vec![None; tasks.len()]);
    let workers = (config.jobs / 4).clamp(1, tasks.len().max(1));
    thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let t = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((i, diff, _)) = tasks.get(t) else {
                        break;
                    };
                    let r = if config.shrink {
                        minimal(area, qt, ljos, &cases[*i], diff, config.budget)
                    } else {
                        (cases[*i].clone(), diff.clone())
                    };
                    out.lock().expect("no shrink panics holding it")[t] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .expect("no shrink panics holding it")
        .into_iter()
        .map(|r| r.expect("every task ran"))
        .collect()
}

/// The smallest case that still differs as `diff` does, and its difference.
fn minimal<A: Area>(
    area: &A,
    qt: &dyn Side,
    ljos: &dyn Side,
    case: &A::Case,
    diff: &Diff,
    budget: Budget,
) -> (A::Case, Diff) {
    let target = coarse(area, case, diff);
    let matching = |c: &A::Case, v: Verdict| match v {
        Verdict::Differ(ds) => ds.into_iter().find(|d| coarse(area, c, d) == target),
        _ => None,
    };
    let small = shrink(area, case, budget, &mut |chunk| {
        let rendered: Vec<Rendered> = chunk.iter().map(|c| area.render(c)).collect();
        let (verdicts, _, _) = judge(qt, ljos, &rendered);
        chunk
            .iter()
            .zip(verdicts)
            .map(|(c, v)| matching(c, v).is_some())
            .collect()
    });
    let (mut verdicts, _, _) = judge(qt, ljos, &[area.render(&small)]);
    match verdicts.pop().and_then(|v| matching(&small, v)) {
        Some(d) => (small, d),
        // Flaky on the way down: keep what was found first.
        None => (case.clone(), diff.clone()),
    }
}
