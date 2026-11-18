//! How a profile's limit is held, and what a run reports of it.
//!
//! A systemd scope is the first choice: the service manager makes a cgroup
//! with the limits and the kernel holds them. Where there is no user
//! manager, as in a container, the bench makes a cgroup of its own where
//! the cgroup tree is writable; failing that it measures in the
//! container's own cgroup when that is held at or below the profile.
//! Limits on the process (`RLIMIT_DATA`, CPU affinity and a stop/continue
//! throttle) hold less and are used only when asked for.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    cgroup::Cgroup,
    profile::{Profile, cpu_set},
};

/// What holds the limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mechanism {
    /// A transient systemd scope, made by the launcher.
    Scope,
    /// A cgroup the bench made under a cgroup it may write.
    Cgroup,
    /// The cgroup the bench and the application are both in, a container's,
    /// held at or below the profile by whoever started the container.
    Container,
    /// Limits set on the application's process: `RLIMIT_DATA`, affinity,
    /// and a throttle.
    Process,
}

impl Mechanism {
    /// The name `--limit-by` takes and the report writes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::Cgroup => "cgroup",
            Self::Container => "container",
            Self::Process => "process",
        }
    }

    /// What holds it, in a sentence.
    #[must_use]
    pub fn by(self) -> &'static str {
        match self {
            Self::Scope => "a systemd scope",
            Self::Cgroup => "a cgroup of its own",
            Self::Container => "the container's cgroup",
            Self::Process => "limits on the process",
        }
    }

    pub const ALL: [Self; 4] = [Self::Scope, Self::Cgroup, Self::Container, Self::Process];

    /// What `auto` tries, in order. Limits on the process are never chosen
    /// for the user: they hold less, so a run under them is asked for.
    pub const AUTO: [Self; 3] = [Self::Scope, Self::Cgroup, Self::Container];
}

/// What `--limit-by` asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LimitBy {
    /// The first of [`Mechanism::AUTO`] that holds the limit.
    #[default]
    Auto,
    Only(Mechanism),
}

impl LimitBy {
    /// # Errors
    /// Not `auto` nor a mechanism's name.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s == "auto" {
            return Ok(Self::Auto);
        }
        Mechanism::ALL
            .into_iter()
            .find(|m| m.name() == s)
            .map(Self::Only)
            .ok_or_else(|| {
                let names: Vec<&str> = Mechanism::ALL.iter().map(|m| m.name()).collect();
                format!("{s}: auto or one of {}", names.join(", "))
            })
    }
}

/// How the CPUs a profile names are held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CpusBy {
    /// The cgroup's `cpuset.cpus`.
    Cpuset,
    /// The application's CPU affinity, which it inherits and could change.
    Affinity,
}

/// Where what the application used is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accounting {
    /// Its own cgroup's files: everything charged to it.
    Cgroup,
    /// Its processes in `/proc` ([`crate::tree`]): what they hold resident,
    /// without the page cache and kernel memory a cgroup also counts.
    Processes,
}

/// How a run's limit was held, as read back once the application was in
/// it: what the run is evidence of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Enforcement {
    pub mechanism: Mechanism,
    /// The application's cgroup, from the root of the tree the bench sees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cgroup: Option<String>,
    /// The limits in force, which may be tighter than the profile's: a
    /// container's own. `None` is none.
    pub memory_max: Option<u64>,
    pub swap_max: Option<u64>,
    /// `[quota, period]` in microseconds.
    pub cpu_max: Option<[u64; 2]>,
    pub cpus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus_by: Option<CpusBy>,
    pub accounting: Accounting,
    /// What of the profile this mechanism does not hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_enforced: Vec<String>,
    /// What else a reader of the numbers should know.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Enforcement {
    /// The limits of the cgroup `dir`, as its files say.
    #[must_use]
    pub fn of_cgroup(root: &Path, dir: &Path, mechanism: Mechanism, cpus_by: CpusBy) -> Self {
        let c = Cgroup {
            dir: dir.to_owned(),
        };
        Self {
            mechanism,
            cgroup: Some(relative(root, dir)),
            memory_max: c.memory_max().and_then(|s| s.parse().ok()),
            swap_max: c.swap_max().and_then(|s| s.parse().ok()),
            cpu_max: c.cpu_max().as_deref().and_then(parse_cpu_max),
            cpus: match cpus_by {
                CpusBy::Cpuset => c.cpus(),
                CpusBy::Affinity => None,
            },
            cpus_by: Some(cpus_by),
            accounting: Accounting::Cgroup,
            not_enforced: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// One line: what holds `profile` and at what.
    #[must_use]
    pub fn describe(&self, profile: &str) -> String {
        let mut out = format!("{profile} is held by {}", self.mechanism.by());
        if let Some(cg) = &self.cgroup {
            let _ = write!(out, " {cg}");
        }
        let mut parts = Vec::new();
        let max = |v: Option<u64>| v.map_or("max".to_owned(), |b| b.to_string());
        let cpu = self
            .cpu_max
            .map_or("max".to_owned(), |[q, p]| format!("{q} {p}"));
        // Without a cgroup there is no memory.max or cpu.max to name, and
        // swap is not held at all.
        if self.mechanism == Mechanism::Process {
            parts.push(format!("RLIMIT_DATA {}", max(self.memory_max)));
            parts.push(format!("CPU {cpu} by stopping its process group"));
        } else {
            parts.push(format!("memory.max {}", max(self.memory_max)));
            parts.push(format!("swap {}", max(self.swap_max)));
            parts.push(format!("cpu.max {cpu}"));
        }
        if let Some(cpus) = &self.cpus {
            let by = match self.cpus_by {
                Some(CpusBy::Affinity) => " by affinity",
                _ => "",
            };
            parts.push(format!("CPUs {cpus}{by}"));
        }
        parts.push(match self.accounting {
            Accounting::Cgroup => "accounted by its cgroup".to_owned(),
            Accounting::Processes => "accounted from its processes in /proc".to_owned(),
        });
        let _ = write!(out, ": {}", parts.join(", "));
        for n in &self.not_enforced {
            let _ = write!(out, "; not held: {n}");
        }
        out
    }
}

/// `dir` under `root`, written as the kernel writes cgroup paths.
#[must_use]
pub fn relative(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    format!("/{}", parts.join("/"))
}

/// `cpu.max`'s `quota period`; `None` for `max`.
#[must_use]
pub fn parse_cpu_max(text: &str) -> Option<[u64; 2]> {
    let (q, p) = text.trim().split_once(' ')?;
    Some([q.parse().ok()?, p.parse().ok()?])
}

/// A limit as found walking up from a cgroup: the tightest one, and the
/// cgroup that sets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bound<T> {
    /// No cgroup on the way has the file.
    Unread,
    /// Every one that has it says `max`; the nearest.
    Max(PathBuf),
    At(T, PathBuf),
}

impl<T: Copy> Bound<T> {
    #[must_use]
    pub fn value(&self) -> Option<T> {
        match self {
            Self::At(v, _) => Some(*v),
            _ => None,
        }
    }

    /// The cgroup that sets it.
    #[must_use]
    pub fn holder(&self) -> Option<&Path> {
        match self {
            Self::Unread => None,
            Self::Max(d) | Self::At(_, d) => Some(d),
        }
    }

    /// `found` in `dir`, kept when tighter by `tighter`.
    fn meet(&mut self, found: Option<T>, dir: &Path, tighter: impl Fn(&T, &T) -> bool) {
        match (&*self, found) {
            (Self::Unread, None) => *self = Self::Max(dir.to_owned()),
            (Self::Unread | Self::Max(_), Some(v)) => *self = Self::At(v, dir.to_owned()),
            (Self::At(have, _), Some(v)) if tighter(&v, have) => {
                *self = Self::At(v, dir.to_owned())
            }
            _ => {}
        }
    }
}

/// The limits a cgroup's processes are held to: the tightest of its own
/// and every cgroup above it up to `root`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effective {
    pub memory: Bound<u64>,
    pub swap: Bound<u64>,
    /// `[quota, period]`.
    pub cpu: Bound<[u64; 2]>,
}

/// Reads [`Effective`] from `cg` up to `root`, both included. A cgroup
/// that is not there (the bench's own path, seen from a tree it is not
/// in) is passed over.
#[must_use]
pub fn effective(root: &Path, cg: &Path) -> Effective {
    let mut out = Effective {
        memory: Bound::Unread,
        swap: Bound::Unread,
        cpu: Bound::Unread,
    };
    let mut dir = cg.to_owned();
    loop {
        let c = Cgroup { dir: dir.clone() };
        if let Some(m) = c.memory_max() {
            out.memory.meet(m.parse().ok(), &dir, |a, b| a < b);
        }
        if let Some(s) = c.swap_max() {
            out.swap.meet(s.parse().ok(), &dir, |a, b| a < b);
        }
        if let Some(cpu) = c.cpu_max() {
            // Compared as rates: quota over period.
            out.cpu.meet(parse_cpu_max(&cpu), &dir, |[q, p], [hq, hp]| {
                u128::from(*q) * u128::from(*hp) < u128::from(*hq) * u128::from(*p)
            });
        }
        if dir == root || !dir.starts_with(root) {
            break;
        }
        match dir.parent() {
            Some(p) => dir = p.to_owned(),
            None => break,
        }
    }
    out
}

/// A percentage as a person writes it: `20`, `12.5`.
#[allow(clippy::cast_precision_loss)]
fn percent(quota: u64, period: u64) -> String {
    let p = quota as f64 * 100.0 / period.max(1) as f64;
    let rounded = (p * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// What podman and docker are told to give a container no swap.
const NO_SWAP_HINT: &str =
    "give the container no swap: podman's and docker's --memory-swap the same as --memory";

/// Whether `eff` holds the application at or below `profile`; what does
/// not, when it does not.
///
/// # Errors
/// The first limit the container lets the application exceed.
pub fn check_container(cg: &Path, eff: &Effective, profile: &Profile) -> Result<(), String> {
    let l = &profile.limits;
    let file = |dir: &Path, name: &str| dir.join(name).display().to_string();
    if let Some(want) = l.memory_max {
        match &eff.memory {
            Bound::At(have, dir) if *have > want => {
                return Err(format!(
                    "{} is {have}, above the {want} asked for",
                    file(dir, "memory.max")
                ));
            }
            Bound::At(..) => {}
            Bound::Max(dir) => {
                return Err(format!(
                    "{} is max, not at most {want}",
                    file(dir, "memory.max")
                ));
            }
            Bound::Unread => {
                return Err(format!(
                    "no memory.max in {} or a cgroup above it",
                    cg.display()
                ));
            }
        }
    }
    if l.no_swap {
        match &eff.swap {
            Bound::At(0, _) => {}
            Bound::At(have, dir) => {
                return Err(format!(
                    "{} is {have}, not 0 ({NO_SWAP_HINT})",
                    file(dir, "memory.swap.max")
                ));
            }
            Bound::Max(dir) => {
                return Err(format!(
                    "{} is max, not 0 ({NO_SWAP_HINT})",
                    file(dir, "memory.swap.max")
                ));
            }
            Bound::Unread if machine_has_swap() => {
                return Err(format!(
                    "no memory.swap.max in {} or a cgroup above it, and the machine has swap",
                    cg.display()
                ));
            }
            Bound::Unread => {}
        }
    }
    if let Some((quota, period)) = profile.cpu_max() {
        let want = percent(quota, period);
        match &eff.cpu {
            Bound::At([q, p], dir)
                if u128::from(*q) * u128::from(period) > u128::from(quota) * u128::from(*p) =>
            {
                return Err(format!(
                    "{} lets it use {} % of one CPU, above the {want} % asked for",
                    file(dir, "cpu.max"),
                    percent(*q, *p)
                ));
            }
            Bound::At(..) => {}
            Bound::Max(dir) => {
                return Err(format!(
                    "{} is max, not at most {want} % of one CPU",
                    file(dir, "cpu.max")
                ));
            }
            Bound::Unread => {
                return Err(format!(
                    "no cpu.max in {} or a cgroup above it",
                    cg.display()
                ));
            }
        }
    }
    Ok(())
}

/// Whether the machine has any swap at all.
#[must_use]
pub fn machine_has_swap() -> bool {
    fs::read_to_string("/proc/meminfo").is_ok_and(|t| {
        t.lines()
            .find_map(|l| l.strip_prefix("SwapTotal:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .is_some_and(|kb| kb > 0)
    })
}

/// The CPUs `pid` may run on (`Cpus_allowed_list`).
#[must_use]
pub fn affinity_of(pid: u32) -> Option<String> {
    let text = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map(|v| v.trim().to_owned())
}

/// The soft `RLIMIT_DATA` of `pid`, in bytes; `None` for unlimited.
#[must_use]
pub fn data_limit_of(pid: u32) -> Option<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/limits")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("Max data size"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// The CPUs this process may run on.
#[must_use]
pub fn own_affinity() -> Option<Vec<u32>> {
    // SAFETY: a zeroed cpu_set_t is an empty set, filled by the call.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(0, size_of::<libc::cpu_set_t>(), &raw mut set) != 0 {
            return None;
        }
        let max = u32::try_from(libc::CPU_SETSIZE).unwrap_or(1024);
        Some(
            (0..max)
                .filter(|&c| libc::CPU_ISSET(c as usize, &set))
                .collect(),
        )
    }
}

/// The set of CPUs `list` names, for [`set_affinity`]; `None` when it
/// names none or one past what a set holds.
#[must_use]
pub fn cpu_mask(list: &str) -> Option<libc::cpu_set_t> {
    let cpus = cpu_set(list)?;
    let max = usize::try_from(libc::CPU_SETSIZE).ok()?;
    if cpus.is_empty() || cpus.iter().any(|&c| c as usize >= max) {
        return None;
    }
    // SAFETY: a zeroed cpu_set_t is an empty set; every CPU is below its
    // size.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for c in cpus {
            libc::CPU_SET(c as usize, &mut set);
        }
        Some(set)
    }
}

/// Holds the calling process to `mask`. Safe to call between fork and
/// exec: one system call.
///
/// # Errors
/// None of the CPUs are ones the process may have.
pub fn set_affinity(mask: &libc::cpu_set_t) -> std::io::Result<()> {
    // SAFETY: the set lives across the call and its size is its own.
    if unsafe { libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), mask) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Holds the calling process's data segment and private mappings to
/// `bytes`. Safe to call between fork and exec: one system call.
///
/// # Errors
/// The limit cannot be lowered.
pub fn set_data_limit(bytes: u64) -> std::io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: bytes,
        rlim_max: bytes,
    };
    // SAFETY: setrlimit(2) reads the struct, which lives across the call.
    if unsafe { libc::setrlimit(libc::RLIMIT_DATA, &raw const limit) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pi_zero() -> Profile {
        // 256 MiB, 20 % of one CPU, on CPU 0.
        Profile::small_board("pi-zero", 256 << 20, 1, 5.0, "0".to_owned())
    }

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        dir
    }

    #[test]
    fn the_tightest_limit_above_a_cgroup_holds_it() {
        let d = tree(&[
            ("memory.max", "268435456\n"),
            ("memory.swap.max", "max\n"),
            ("cpu.max", "max 100000\n"),
            ("app/memory.max", "max\n"),
            ("app/memory.swap.max", "0\n"),
            ("app/cpu.max", "20000 100000\n"),
            ("app/x/memory.max", "536870912\n"),
        ]);
        let root = d.path();
        let eff = effective(root, &root.join("app/x/gone"));
        assert_eq!(eff.memory, Bound::At(256 << 20, root.to_owned()));
        assert_eq!(eff.swap, Bound::At(0, root.join("app")));
        assert_eq!(eff.cpu, Bound::At([20000, 100_000], root.join("app")));
        assert_eq!(check_container(root, &eff, &pi_zero()), Ok(()));
    }

    #[test]
    fn a_container_held_tighter_is_accepted_and_one_held_looser_is_not() {
        let d = tree(&[
            ("memory.max", "134217728\n"),
            ("memory.swap.max", "0\n"),
            ("cpu.max", "1000 10000\n"),
        ]);
        let root = d.path();
        let eff = effective(root, root);
        assert_eq!(check_container(root, &eff, &pi_zero()), Ok(()));

        let d = tree(&[
            ("memory.max", "268435456\n"),
            ("memory.swap.max", "0\n"),
            ("cpu.max", "25000 100000\n"),
        ]);
        let root = d.path();
        let why = check_container(root, &effective(root, root), &pi_zero()).unwrap_err();
        assert!(
            why.ends_with("cpu.max lets it use 25 % of one CPU, above the 20 % asked for"),
            "{why}"
        );
    }

    #[test]
    fn a_container_with_swap_is_refused_with_what_to_change() {
        let d = tree(&[
            ("memory.max", "268435456\n"),
            ("memory.swap.max", "268435456\n"),
            ("cpu.max", "20000 100000\n"),
        ]);
        let root = d.path();
        let why = check_container(root, &effective(root, root), &pi_zero()).unwrap_err();
        assert!(why.contains("memory.swap.max is 268435456, not 0"), "{why}");
        assert!(why.contains("--memory-swap"), "{why}");
    }

    #[test]
    fn the_desktop_profile_holds_nothing_to_check() {
        let d = tree(&[]);
        let eff = effective(d.path(), d.path());
        assert_eq!(eff.memory, Bound::Unread);
        assert_eq!(check_container(d.path(), &eff, &Profile::desktop()), Ok(()));
    }

    #[test]
    fn percentages_are_written_as_people_write_them() {
        assert_eq!(percent(2000, 10000), "20");
        assert_eq!(percent(12500, 100_000), "12.5");
    }

    #[test]
    fn a_description_names_the_limits_as_the_mechanism_holds_them() {
        let mut e = Enforcement {
            mechanism: Mechanism::Container,
            cgroup: Some("/".to_owned()),
            memory_max: Some(268_435_456),
            swap_max: Some(0),
            cpu_max: Some([20000, 100_000]),
            cpus: Some("0".to_owned()),
            cpus_by: Some(CpusBy::Affinity),
            accounting: Accounting::Processes,
            not_enforced: Vec::new(),
            notes: Vec::new(),
        };
        assert_eq!(
            e.describe("pi-zero"),
            "pi-zero is held by the container's cgroup /: memory.max 268435456, swap 0, cpu.max 20000 100000, CPUs 0 by affinity, accounted from its processes in /proc"
        );
        e.mechanism = Mechanism::Process;
        e.cgroup = None;
        e.swap_max = None;
        e.not_enforced = vec!["swap is not held back".to_owned()];
        assert_eq!(
            e.describe("pi-zero"),
            "pi-zero is held by limits on the process: RLIMIT_DATA 268435456, CPU 20000 100000 by stopping its process group, CPUs 0 by affinity, accounted from its processes in /proc; not held: swap is not held back"
        );
    }

    #[test]
    fn limit_by_names() {
        assert_eq!(LimitBy::parse("auto"), Ok(LimitBy::Auto));
        assert_eq!(
            LimitBy::parse("container"),
            Ok(LimitBy::Only(Mechanism::Container))
        );
        assert!(LimitBy::parse("rlimit").is_err());
    }

    #[test]
    fn a_cgroup_path_is_written_from_the_root() {
        let root = Path::new("/sys/fs/cgroup");
        assert_eq!(relative(root, root), "/");
        assert_eq!(relative(root, &root.join("a/b")), "/a/b");
    }

    #[test]
    fn this_process_reads_its_own_limits() {
        let me = std::process::id();
        let cpus = affinity_of(me).unwrap();
        assert_eq!(
            cpu_set(&cpus),
            own_affinity().map(|v| v.into_iter().collect::<Vec<_>>())
        );
        assert!(cpu_mask("0").is_some());
        assert!(cpu_mask("").is_none());
    }

    #[test]
    fn a_child_holds_the_limits_set_before_its_exec() {
        use std::os::unix::process::CommandExt;
        let first = own_affinity().unwrap()[0];
        let mask = cpu_mask(&first.to_string()).unwrap();
        let mut command = std::process::Command::new("sleep");
        command.arg("5");
        // SAFETY: two system calls between fork and exec.
        unsafe {
            command.pre_exec(move || {
                set_data_limit(64 << 20)?;
                set_affinity(&mask)
            });
        }
        let mut child = command.spawn().unwrap();
        let (data, cpus) = (data_limit_of(child.id()), affinity_of(child.id()));
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(data, Some(64 << 20));
        assert_eq!(cpus, Some(first.to_string()));
    }
}
