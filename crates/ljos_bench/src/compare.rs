//! A report against an earlier one (`ljos bench --compare`): each cell
//! that moved by more than runs of the same build move between them.
//!
//! The bench has no floors, so a comparison does not pass or fail; it says
//! where a runtime got worse or better since the report it is compared
//! with, cell by cell, by the median of each cell's repeats.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::{Gesture, Report, Run};

/// How far a value may move between runs of the same build before a
/// comparison says it moved: `relative` of the earlier value plus
/// `absolute`, in the metric's unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margin {
    pub relative: f64,
    pub absolute: f64,
}

impl Margin {
    #[must_use]
    pub fn of(self, base: f64) -> f64 {
        base.abs() * self.relative + self.absolute
    }
}

/// One metric compared: lower is better for every one of them.
struct Check {
    name: &'static str,
    unit: &'static str,
    value: fn(&Run) -> Option<f64>,
    margin: Margin,
}

const MIB: f64 = 1_048_576.0;

/// The margins are the spread docs/BENCHMARKS.md ("Noise", and the bold
/// rule of the gallery tables) saw between repeats of one build on a
/// shared machine: timings move with the machine's load, memory with the
/// page cache.
#[allow(clippy::cast_precision_loss)]
const CHECKS: &[Check] = &[
    Check {
        name: "first frame",
        unit: "ms",
        value: |r| r.first_frame_ms,
        margin: Margin {
            relative: 0.25,
            absolute: 20.0,
        },
    },
    Check {
        name: "first frame that answers",
        unit: "ms",
        value: |r| r.input_ready_ms,
        margin: Margin {
            relative: 0.25,
            absolute: 20.0,
        },
    },
    Check {
        name: "settled memory",
        unit: "MiB",
        value: |r| r.settled.as_ref()?.memory_current.map(|b| b as f64 / MIB),
        margin: Margin {
            relative: 0.10,
            absolute: 2.0,
        },
    },
    Check {
        name: "settled private memory",
        unit: "MiB",
        value: |r| {
            r.settled
                .as_ref()?
                .resident
                .as_ref()
                .map(|p| p.pss_anon as f64 / MIB)
        },
        margin: Margin {
            relative: 0.10,
            absolute: 2.0,
        },
    },
    Check {
        name: "idle CPU",
        unit: "% of a core",
        value: |r| r.idle.as_ref().map(|s| s.cpu_percent),
        margin: Margin {
            relative: 0.10,
            absolute: 0.1,
        },
    },
    Check {
        name: "driven CPU",
        unit: "% of a core",
        value: |r| r.load.as_ref().map(|s| s.cpu_percent),
        margin: Margin {
            relative: 0.10,
            absolute: 0.5,
        },
    },
    Check {
        name: "driven busiest thread",
        unit: "% of a core",
        value: |r| {
            let load = r.load.as_ref()?;
            let busiest = load.busiest_thread.as_ref()?;
            (load.seconds > 0.0).then(|| busiest.cpu_ms / 10.0 / load.seconds)
        },
        margin: Margin {
            relative: 0.10,
            absolute: 0.5,
        },
    },
    Check {
        name: "driven frame interval p95",
        unit: "ms",
        value: |r| r.load.as_ref()?.frames.as_ref()?.p95_ms,
        margin: Margin {
            relative: 0.25,
            absolute: 8.0,
        },
    },
    Check {
        name: "lag p50",
        unit: "ms",
        value: |r| r.load.as_ref()?.frames.as_ref()?.latency_p50_ms,
        margin: Margin {
            relative: 0.25,
            absolute: 4.0,
        },
    },
    Check {
        name: "lag p95",
        unit: "ms",
        value: |r| r.load.as_ref()?.frames.as_ref()?.latency_p95_ms,
        margin: Margin {
            relative: 0.25,
            absolute: 8.0,
        },
    },
];

/// Which way a cell moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Higher by more than the margin.
    Worse,
    /// Lower by more than the margin.
    Better,
    /// Within the margin.
    Same,
}

/// One metric of one application on one runtime, profile and gesture.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub app: String,
    pub runtime: String,
    pub profile: String,
    pub gesture: Gesture,
    pub metric: String,
    pub unit: String,
    /// The medians of the earlier report's repeats and of this one's.
    pub base: f64,
    pub now: f64,
    /// How far it may move before it counts.
    pub margin: f64,
    pub verdict: Verdict,
}

/// Every cell both reports have, and what only the earlier one had.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Comparison {
    pub base_date: String,
    pub now_date: String,
    pub cells: Vec<Cell>,
    /// `app runtime profile gesture` of each run the earlier report had
    /// and this one has not, and `app runtime profile gesture: metric` of
    /// each value it had and this one's runs have none of (the app never
    /// drew, no frame was seen): not measured, so not compared. A metric
    /// that is gone may be the worst a run can do, so it is said.
    pub missing: Vec<String>,
}

impl Comparison {
    #[must_use]
    pub fn worse(&self) -> Vec<&Cell> {
        self.cells
            .iter()
            .filter(|c| c.verdict == Verdict::Worse)
            .collect()
    }
}

type Key<'a> = (&'a str, &'a str, &'a str, Gesture);

fn key(r: &Run) -> Key<'_> {
    (&r.app, &r.runtime, &r.profile, r.gesture)
}

fn keys(report: &Report) -> Vec<Key<'_>> {
    let mut out: Vec<Key<'_>> = Vec::new();
    for r in &report.runs {
        if !out.contains(&key(r)) {
            out.push(key(r));
        }
    }
    out
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        f64::midpoint(v[n / 2 - 1], v[n / 2])
    })
}

fn value(report: &Report, k: Key<'_>, check: &Check) -> Option<f64> {
    median(
        report
            .runs
            .iter()
            .filter(|r| key(r) == k)
            .filter_map(|r| (check.value)(r))
            .collect(),
    )
}

/// `now` against `base`, cell by cell.
#[must_use]
pub fn compare(base: &Report, now: &Report) -> Comparison {
    let now_keys = keys(now);
    let mut out = Comparison {
        base_date: base.date.clone(),
        now_date: now.date.clone(),
        ..Comparison::default()
    };
    for k in keys(base) {
        if !now_keys.contains(&k) {
            out.missing
                .push(format!("{} {} {} {}", k.0, k.1, k.2, k.3.name()));
            continue;
        }
        for check in CHECKS {
            let Some(b) = value(base, k, check) else {
                continue;
            };
            let Some(n) = value(now, k, check) else {
                out.missing.push(format!(
                    "{} {} {} {}: {}",
                    k.0,
                    k.1,
                    k.2,
                    k.3.name(),
                    check.name
                ));
                continue;
            };
            let margin = check.margin.of(b);
            let verdict = if n > b + margin {
                Verdict::Worse
            } else if n < b - margin {
                Verdict::Better
            } else {
                Verdict::Same
            };
            out.cells.push(Cell {
                app: k.0.to_owned(),
                runtime: k.1.to_owned(),
                profile: k.2.to_owned(),
                gesture: k.3,
                metric: check.name.to_owned(),
                unit: check.unit.to_owned(),
                base: b,
                now: n,
                margin,
                verdict,
            });
        }
    }
    out
}

fn num(v: f64) -> String {
    if v.abs() >= 100.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

/// The comparison as markdown: the cells that got worse, then those that
/// got better, then what was not measured again.
#[must_use]
pub fn markdown(c: &Comparison) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Against the report of {}\n\nThis report ({}) against the earlier one, by the median of each cell's repeats. A cell counts as moved when it is higher or lower by more than the margin: what runs of one build moved by between them (docs/BENCHMARKS.md, \"Noise\").\n",
        c.base_date, c.now_date
    );
    for (title, verdict) in [("Worse", Verdict::Worse), ("Better", Verdict::Better)] {
        let cells: Vec<&Cell> = c.cells.iter().filter(|x| x.verdict == verdict).collect();
        let _ = writeln!(out, "## {title} ({})\n", cells.len());
        if cells.is_empty() {
            out.push_str("None.\n\n");
            continue;
        }
        out.push_str(
            "| App | Runtime | Profile | Gesture | Metric | Before | Now | Margin |\n|---|---|---|---|---|---:|---:|---:|\n",
        );
        for x in cells {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} ({}) | {} | {} | ±{} |",
                x.app,
                x.runtime,
                x.profile,
                x.gesture.name(),
                x.metric,
                x.unit,
                num(x.base),
                num(x.now),
                num(x.margin)
            );
        }
        out.push('\n');
    }
    let same = c
        .cells
        .iter()
        .filter(|x| x.verdict == Verdict::Same)
        .count();
    let _ = writeln!(out, "{same} cells within their margin.\n");
    if !c.missing.is_empty() {
        let _ = writeln!(out, "## Not measured again\n");
        for m in &c.missing {
            let _ = writeln!(out, "- {m}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_margin_is_part_of_the_value_and_a_floor() {
        let m = Margin {
            relative: 0.25,
            absolute: 20.0,
        };
        assert!((m.of(100.0) - 45.0).abs() < 1e-9);
        assert!((m.of(0.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_metric_the_new_run_lacks_is_listed_as_not_measured() {
        let base: Report =
            serde_json::from_str(include_str!("../../../bench/baseline/desktop.json")).unwrap();
        let mut now = base.clone();
        let app = now.runs[0].app.clone();
        for r in now.runs.iter_mut().filter(|r| r.app == app) {
            r.first_frame_ms = None;
        }
        let c = compare(&base, &now);
        let lost = format!(
            "{app} {} {} {}: first frame",
            now.runs[0].runtime,
            now.runs[0].profile,
            now.runs[0].gesture.name()
        );
        assert!(c.missing.contains(&lost), "{:?}", c.missing);
        assert!(c.worse().is_empty());
        assert!(markdown(&c).contains(&format!("- {lost}")));
    }
}
