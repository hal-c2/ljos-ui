//! The limits an application is measured under, and how a launcher is
//! asked for them.

use serde::{Deserialize, Serialize};

/// How many of this machine's CPU-seconds one core of the modelled device
/// does in a second, as a divisor: 20 means a device core is a twentieth of
/// one of ours. A per-core quota of `100 / factor` percent is given to each
/// of `cpus` CPUs.
pub const DEFAULT_CPU_FACTOR: f64 = 20.0;
/// The cgroup period the quota is counted over. Ten milliseconds rather
/// than systemd's default hundred, so a throttled application stalls for at
/// most a few milliseconds at a time, as a slow core would, instead of
/// running at full speed and then not at all for most of a frame.
pub const QUOTA_PERIOD_MS: u32 = 10;
/// The pi-zero default: 512 MB less what the OS, its compositor and
/// services keep.
pub const DEFAULT_MEMORY_MAX: u64 = 256 << 20;

/// What a profile limits. `None` is no limit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub memory_max: Option<u64>,
    /// No swap: what does not fit in memory is not there.
    pub no_swap: bool,
    /// The CPU-seconds a second the whole application may use, in percent
    /// of one CPU of this machine.
    pub cpu_quota_percent: Option<u32>,
    /// The CPUs it may run on, as cgroups write them (`0-3`, `8,10`).
    pub cpus: Option<String>,
    /// The factor the quota was worked out from, for the report.
    pub cpu_factor: Option<f64>,
}

/// A named set of limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub limits: Limits,
}

impl Profile {
    /// No limit: the application in a scope of its own only so that its
    /// use can be read.
    #[must_use]
    pub fn desktop() -> Self {
        Self {
            name: "desktop".to_owned(),
            limits: Limits {
                memory_max: None,
                no_swap: false,
                cpu_quota_percent: None,
                cpus: None,
                cpu_factor: None,
            },
        }
    }

    /// A small board: `cpus` cores, each `cpu_factor` times slower than
    /// one of this machine's, on the CPUs of `cpu_list`, and `memory_max`
    /// bytes with no swap.
    #[must_use]
    pub fn small_board(
        name: &str,
        memory_max: u64,
        cpus: u32,
        cpu_factor: f64,
        cpu_list: String,
    ) -> Self {
        let percent = (f64::from(cpus) * 100.0 / cpu_factor).round().max(1.0);
        Self {
            name: name.to_owned(),
            limits: Limits {
                memory_max: Some(memory_max),
                no_swap: true,
                // At most a few thousand percent, whatever the factor.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                cpu_quota_percent: Some(percent as u32),
                cpus: Some(cpu_list),
                cpu_factor: Some(cpu_factor),
            },
        }
    }

    /// The same profile with another memory limit, for the search for the
    /// least an application starts in.
    #[must_use]
    pub fn with_memory_max(&self, bytes: u64) -> Self {
        let mut out = self.clone();
        out.limits.memory_max = Some(bytes);
        out.limits.no_swap = true;
        out
    }

    /// `systemd-run`'s arguments for a transient scope with these limits,
    /// up to the command.
    #[must_use]
    pub fn launcher_args(&self) -> Vec<String> {
        let l = &self.limits;
        let mut args: Vec<String> = ["--user", "--scope", "--quiet"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        if let Some(bytes) = l.memory_max {
            args.push(format!("--property=MemoryMax={bytes}"));
        }
        if l.no_swap {
            args.push("--property=MemorySwapMax=0".to_owned());
        }
        if let Some(cpus) = &l.cpus {
            args.push(format!("--property=AllowedCPUs={cpus}"));
        }
        if let Some(percent) = l.cpu_quota_percent {
            args.push(format!("--property=CPUQuota={percent}%"));
            args.push(format!("--property=CPUQuotaPeriodSec={QUOTA_PERIOD_MS}ms"));
        }
        args
    }

    /// What the scope's `cpu.max` reads once the quota is applied:
    /// microseconds of CPU per period, and the period.
    #[must_use]
    pub fn cpu_max(&self) -> Option<(u64, u64)> {
        let period = u64::from(QUOTA_PERIOD_MS) * 1000;
        self.limits
            .cpu_quota_percent
            .map(|p| (u64::from(p) * period / 100, period))
    }
}

/// `count` CPUs on distinct physical cores, from the highest-numbered core
/// down, so SMT siblings never share the device's cores and the CPUs the
/// desktop starts on first are left alone. Read from sysfs; `None` when it
/// cannot be.
#[must_use]
pub fn distinct_cores(count: u32) -> Option<String> {
    let mut by_core = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        let name = entry.file_name().into_string().ok()?;
        let Some(n) = name.strip_prefix("cpu").and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let core = std::fs::read_to_string(entry.path().join("topology/core_id")).ok()?;
        let package =
            std::fs::read_to_string(entry.path().join("topology/physical_package_id")).ok()?;
        let key = (package.trim().to_owned(), core.trim().parse::<u32>().ok()?);
        by_core
            .entry(key)
            .and_modify(|cpu: &mut u32| *cpu = (*cpu).min(n))
            .or_insert(n);
    }
    let mut cpus: Vec<u32> = by_core.into_values().collect();
    cpus.sort_unstable();
    let count = usize::try_from(count).ok()?;
    if cpus.len() < count {
        return None;
    }
    let mut chosen = cpus.split_off(cpus.len() - count);
    chosen.sort_unstable();
    Some(
        chosen
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
    )
}

/// The CPUs of a cgroup list (`0-3,8`), for comparing two ways of writing
/// the same set.
#[must_use]
pub fn cpu_set(list: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for part in list.trim().split(',').filter(|p| !p.is_empty()) {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (a.parse::<u32>().ok()?, b.parse::<u32>().ok()?);
                out.extend(a..=b);
            }
            None => out.push(part.parse().ok()?),
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pi_zero_is_four_twentieths() {
        let p = Profile::small_board("pi-zero", 64 << 20, 4, 20.0, "0-3".to_owned());
        assert_eq!(p.limits.cpu_quota_percent, Some(20));
        assert_eq!(p.cpu_max(), Some((2000, 10000)));
        assert_eq!(
            p.launcher_args()[3..],
            [
                "--property=MemoryMax=67108864",
                "--property=MemorySwapMax=0",
                "--property=AllowedCPUs=0-3",
                "--property=CPUQuota=20%",
                "--property=CPUQuotaPeriodSec=10ms",
            ]
        );
    }

    #[test]
    fn the_desktop_asks_for_nothing() {
        assert_eq!(
            Profile::desktop().launcher_args(),
            ["--user", "--scope", "--quiet"]
        );
    }

    #[test]
    fn cpu_lists_compare_as_sets() {
        assert_eq!(cpu_set("0-3"), cpu_set("0,1,2,3"));
        assert_eq!(cpu_set("8,10\n"), Some(vec![8, 10]));
        assert_eq!(cpu_set("x"), None);
    }
}
