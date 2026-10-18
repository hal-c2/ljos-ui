//! The report: everything measured as JSON, and the same as markdown
//! tables of Ljós against Qt.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::{
    measure::{MinMemory, Outcome, Run, Span},
    profile::Profile,
};

/// The machine the numbers were taken on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub cpu: String,
    pub cpus: usize,
    pub memory_bytes: u64,
    pub kernel: String,
}

impl Machine {
    #[must_use]
    pub fn this() -> Self {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        Self {
            cpu: cpuinfo
                .lines()
                .find_map(|l| l.strip_prefix("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_owned())
                .unwrap_or_default(),
            cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
            memory_bytes: meminfo
                .lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
                .map_or(0, |kb| kb * 1024),
            kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_default()
                .trim()
                .to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    /// The day the run started, UTC.
    pub date: String,
    pub machine: Machine,
    pub profiles: Vec<Profile>,
    /// Free text: how the runtimes were built and run.
    pub notes: Vec<String>,
    pub runs: Vec<Run>,
    pub min_memory: Vec<MinMemory>,
}

impl Report {
    /// An empty report of a run starting now on this machine.
    #[must_use]
    pub fn new(profiles: Vec<Profile>, notes: Vec<String>) -> Self {
        Self {
            date: today(),
            machine: Machine::this(),
            profiles,
            notes,
            runs: Vec::new(),
            min_memory: Vec::new(),
        }
    }

    /// Takes in the profiles and notes of a run added to this report: one
    /// long run made in parts keeps one report.
    pub fn extend(&mut self, profiles: &[Profile], notes: &[String]) {
        for p in profiles {
            if let Some(old) = self.profiles.iter_mut().find(|o| o.name == p.name) {
                *old = p.clone();
            } else {
                self.profiles.push(p.clone());
            }
        }
        for n in notes {
            if !self.notes.contains(n) {
                self.notes.push(n.clone());
            }
        }
    }
}

/// Today, UTC, as `YYYY-MM-DD`.
#[must_use]
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 as a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (y, m as u32, d as u32)
}

const MIB: f64 = 1_048_576.0;

/// A column of the tables: its title and how a run gives its value.
struct Metric {
    title: &'static str,
    value: fn(&Run) -> Option<f64>,
}

#[allow(clippy::cast_precision_loss)]
const METRICS: &[Metric] = &[
    Metric {
        title: "Time to first frame (ms)",
        value: |r| r.first_frame_ms,
    },
    Metric {
        title: "Time to first frame that answers input (ms)",
        value: |r| r.input_ready_ms,
    },
    Metric {
        title: "CPU time to first frame (ms)",
        value: |r| r.first_frame_ms.and(r.startup_cpu_ms),
    },
    Metric {
        title: "Peak memory while starting (MiB, cgroup)",
        value: |r| r.peak_memory.map(|b| b as f64 / MIB),
    },
    Metric {
        title: "Settled memory (MiB, cgroup memory.current)",
        value: |r| r.settled.as_ref()?.memory_current.map(|b| b as f64 / MIB),
    },
    Metric {
        title: "Settled private memory (MiB, PSS of anonymous pages)",
        value: |r| {
            r.settled
                .as_ref()?
                .resident
                .as_ref()
                .map(|p| p.pss_anon as f64 / MIB)
        },
    },
    Metric {
        title: "Settled PSS (MiB, all pages)",
        value: |r| {
            r.settled
                .as_ref()?
                .resident
                .as_ref()
                .map(|p| p.pss as f64 / MIB)
        },
    },
    Metric {
        title: "Memory after idling (MiB, cgroup memory.current)",
        value: |r| r.idled.as_ref()?.memory_current.map(|b| b as f64 / MIB),
    },
    Metric {
        title: "PSS after idling (MiB, all pages)",
        value: |r| {
            r.idled
                .as_ref()?
                .resident
                .as_ref()
                .map(|p| p.pss as f64 / MIB)
        },
    },
    Metric {
        title: "Idle CPU (% of one core)",
        value: |r| r.idle.as_ref().map(|s| s.cpu_percent),
    },
    Metric {
        title: "Idle CPU of threads that ended (% of one core)",
        value: |r| {
            let s = r.idle.as_ref()?;
            (s.seconds > 0.0).then(|| s.ended_cpu_ms.unwrap_or(0.0) / 10.0 / s.seconds)
        },
    },
    Metric {
        title: "Idle wakeups (per second)",
        value: |r| r.idle.as_ref().map(|s| s.wakeups_per_s),
    },
    Metric {
        title: "Frames shown while idle (per second; animations)",
        value: |r| r.idle.as_ref()?.frames.as_ref().map(|f| f.fps),
    },
    Metric {
        title: "Frames sent while idle, changed or not (per second)",
        value: |r| sent_per_s(r.idle.as_ref()?),
    },
    Metric {
        title: "95th percentile frame interval while idle (ms; animations)",
        value: |r| r.idle.as_ref()?.frames.as_ref()?.p95_ms,
    },
    Metric {
        title: "Slowest frame interval while idle (ms; animations)",
        value: |r| r.idle.as_ref()?.frames.as_ref()?.max_ms,
    },
    Metric {
        title: "Wait before driving until nothing changed for 300 ms (ms)",
        value: |r| r.quiet_ms,
    },
    Metric {
        title: "CPU while driven (% of one core)",
        value: |r| r.load.as_ref().map(|s| s.cpu_percent),
    },
    Metric {
        title: "Frames shown while driven (per second)",
        value: |r| r.load.as_ref()?.frames.as_ref().map(|f| f.fps),
    },
    Metric {
        title: "Frames sent while driven, changed or not (per second)",
        value: |r| sent_per_s(r.load.as_ref()?),
    },
    Metric {
        title: "Median frame interval while driven (ms)",
        value: |r| r.load.as_ref()?.frames.as_ref()?.p50_ms,
    },
    Metric {
        title: "95th percentile frame interval while driven (ms)",
        value: |r| r.load.as_ref()?.frames.as_ref()?.p95_ms,
    },
    Metric {
        title: "Median input-to-frame latency while driven (ms)",
        value: |r| r.load.as_ref()?.frames.as_ref()?.latency_p50_ms,
    },
    Metric {
        title: "95th percentile input-to-frame latency while driven (ms)",
        value: |r| r.load.as_ref()?.frames.as_ref()?.latency_p95_ms,
    },
    Metric {
        title: "Slowest frame interval while driven (ms)",
        value: |r| r.load.as_ref()?.frames.as_ref()?.max_ms,
    },
    Metric {
        title: "Mapped install (MiB on disk: runtime, libraries, plugins, fonts)",
        value: |r| {
            r.install
                .as_ref()
                .map(|i| i.groups.values().map(|g| g.bytes).sum::<u64>() as f64 / MIB)
        },
    },
];

/// Frames the client sent over `span`, whether they changed a pixel or not.
#[allow(clippy::cast_precision_loss)]
fn sent_per_s(span: &Span) -> Option<f64> {
    let sent = span.frames.as_ref()?.committed?;
    (span.seconds > 0.0).then(|| sent as f64 / span.seconds)
}

fn cell(v: Option<f64>) -> String {
    match v {
        None => "—".to_owned(),
        Some(v) if v.abs() >= 100.0 => format!("{v:.0}"),
        Some(v) if v.abs() >= 10.0 => format!("{v:.1}"),
        Some(v) => format!("{v:.2}"),
    }
}

/// The median of the values, the mean of the middle two for an even count.
fn median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut v: Vec<f64> = values.collect();
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

fn mark(run: &Run) -> &'static str {
    match run.outcome {
        Outcome::Ok => "",
        Outcome::NoFrame => " (no frame)",
        Outcome::OomKilled => " (OOM)",
        Outcome::Exited => " (exited)",
    }
}

/// The report as markdown: one table per profile and metric, each runtime
/// a column and its ratio to Qt beside it, and the least memory each
/// started in.
#[must_use]
pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# Ljós vs Qt, {}\n\n{} ({} CPUs, {:.0} GiB), Linux {}.\n",
        report.date,
        report.machine.cpu,
        report.machine.cpus,
        report.machine.memory_bytes as f64 / (MIB * 1024.0),
        report.machine.kernel
    );
    for note in &report.notes {
        let _ = writeln!(out, "- {note}");
    }
    let loads: Vec<f64> = report.runs.iter().filter_map(|r| r.load_average).collect();
    if let (Some(lo), Some(hi)) = (
        loads.iter().copied().reduce(f64::min),
        loads.iter().copied().reduce(f64::max),
    ) {
        let repeats = report.runs.iter().map(|r| r.repeat + 1).max().unwrap_or(1);
        let _ = writeln!(
            out,
            "- Each cell is the median of {repeats} runs. The machine's one-minute load average was {lo:.1} to {hi:.1} while they ran (`load_average` of each run in report.json)."
        );
    }
    let mut runtimes: Vec<&str> = Vec::new();
    for r in &report.runs {
        if !runtimes.contains(&r.runtime.as_str()) {
            runtimes.push(&r.runtime);
        }
    }
    // Qt last: every other runtime is a ratio to it.
    runtimes.sort_by_key(|r| *r == "qt");
    let others: Vec<&str> = runtimes.iter().copied().filter(|r| *r != "qt").collect();
    let has_qt = runtimes.contains(&"qt");
    for profile in &report.profiles {
        let runs: Vec<&Run> = report
            .runs
            .iter()
            .filter(|r| r.profile == profile.name)
            .collect();
        if runs.is_empty() {
            continue;
        }
        let mut apps: Vec<&str> = Vec::new();
        for r in &runs {
            if !apps.contains(&r.app.as_str()) {
                apps.push(&r.app);
            }
        }
        let l = &profile.limits;
        let _ = writeln!(
            out,
            "\n## Profile `{}`\n\nMemory {}, swap {}, CPU {}{}.\n",
            profile.name,
            l.memory_max
                .map_or("unlimited".to_owned(), |b| format!("{} MiB", b >> 20)),
            if l.no_swap { "none" } else { "as the machine" },
            l.cpu_quota_percent
                .map_or("unlimited".to_owned(), |p| format!("{p} % of one core")),
            l.cpus
                .as_ref()
                .map_or(String::new(), |c| format!(" on CPUs {c}")),
        );
        for metric in METRICS {
            let _ = writeln!(out, "### {}\n", metric.title);
            let mut head = String::from("| App |");
            let mut rule = String::from("|---|");
            for rt in &runtimes {
                let _ = write!(head, " {rt} |");
                rule.push_str("---:|");
            }
            if has_qt {
                for rt in &others {
                    let _ = write!(head, " {rt}/qt |");
                    rule.push_str("---:|");
                }
            }
            let _ = writeln!(out, "{head}\n{rule}");
            let mut logs: Vec<Vec<f64>> = vec![Vec::new(); others.len()];
            for app in &apps {
                let of = |rt: &str| -> Vec<&Run> {
                    runs.iter()
                        .copied()
                        .filter(|r| r.app == *app && r.runtime == rt)
                        .collect()
                };
                let value = |rt: &str| median(of(rt).iter().filter_map(|r| (metric.value)(r)));
                let mut row = format!("| {app} |");
                for rt in &runtimes {
                    let all = of(rt);
                    let text = if all.is_empty() {
                        "—".to_owned()
                    } else {
                        // A run that did not end well is said, even when
                        // another repeat did.
                        let flag = all
                            .iter()
                            .map(|r| mark(r))
                            .find(|m| !m.is_empty())
                            .unwrap_or("");
                        format!("{}{flag}", cell(value(rt)))
                    };
                    let _ = write!(row, " {text} |");
                }
                if has_qt {
                    let qt = value("qt");
                    for (i, rt) in others.iter().enumerate() {
                        let mine = value(rt);
                        let ratio = mine.zip(qt).and_then(|(a, b)| (b > 0.0).then(|| a / b));
                        if let Some(x) = ratio.filter(|x| *x > 0.0) {
                            logs[i].push(x.ln());
                        }
                        let _ = write!(
                            row,
                            " {} |",
                            ratio.map_or("—".to_owned(), |x| format!("×{x:.2}"))
                        );
                    }
                }
                let _ = writeln!(out, "{row}");
            }
            if has_qt && logs.iter().any(|l| !l.is_empty()) {
                let mut row = String::from("| *geometric mean* |");
                for _ in &runtimes {
                    row.push_str(" |");
                }
                for l in &logs {
                    #[allow(clippy::cast_precision_loss)]
                    let mean =
                        (!l.is_empty()).then(|| (l.iter().sum::<f64>() / l.len() as f64).exp());
                    let _ = write!(
                        row,
                        " {} |",
                        mean.map_or("—".to_owned(), |x| format!("×{x:.2}"))
                    );
                }
                let _ = writeln!(out, "{row}");
            }
            out.push('\n');
        }
    }
    if !report.min_memory.is_empty() {
        let _ = writeln!(
            out,
            "## The least memory each starts in\n\nThe smallest `MemoryMax` (MiB, no swap) at which the first frame was shown and the application still ran a second later, under the profile's CPU limit. Pages of shared libraries another process already brought in are not charged to it (see docs/BENCHMARKS.md).\n"
        );
        let mut head = String::from("| App | Profile |");
        let mut rule = String::from("|---|---|");
        for rt in &runtimes {
            let _ = write!(head, " {rt} |");
            rule.push_str("---:|");
        }
        let _ = writeln!(out, "{head}\n{rule}");
        let mut keys: Vec<(&str, &str)> = Vec::new();
        for m in &report.min_memory {
            if !keys.contains(&(m.app.as_str(), m.profile.as_str())) {
                keys.push((&m.app, &m.profile));
            }
        }
        for (app, profile) in keys {
            let mut row = format!("| {app} | {profile} |");
            for rt in &runtimes {
                let m = report
                    .min_memory
                    .iter()
                    .find(|m| m.app == app && m.profile == profile && m.runtime == *rt);
                let text = match m {
                    None => "—".to_owned(),
                    Some(m) => m.mib.map_or("> max".to_owned(), |v| v.to_string()),
                };
                let _ = write!(row, " {text} |");
            }
            let _ = writeln!(out, "{row}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_376), (2025, 10, 15));
        assert_eq!(civil(11_016), (2000, 2, 29));
    }
}
