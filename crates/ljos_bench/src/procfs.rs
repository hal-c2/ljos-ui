//! What `/proc` says about a process: its resident memory split by kind,
//! its threads and wakeups, and the files it has mapped.

use std::{collections::BTreeMap, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

/// `smaps_rollup`, in bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resident {
    pub rss: u64,
    pub pss: u64,
    pub pss_anon: u64,
    pub pss_file: u64,
    pub pss_shmem: u64,
    /// The process's share of its pages swapped out (`SwapPss`). The
    /// machine may swap an idle application out under pressure from
    /// other work, which takes pages out of `rss` and `pss_anon` without
    /// the application having given them back.
    #[serde(default)]
    pub swap_pss: u64,
}

/// # Errors
/// The process is gone.
pub fn resident(pid: u32) -> std::io::Result<Resident> {
    let text = fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))?;
    let kb = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map_or(0, |v| v * 1024)
    };
    Ok(Resident {
        rss: kb("Rss"),
        pss: kb("Pss"),
        pss_anon: kb("Pss_Anon"),
        pss_file: kb("Pss_File"),
        pss_shmem: kb("Pss_Shmem"),
        swap_pss: kb("SwapPss"),
    })
}

/// The process's threads.
#[must_use]
pub fn threads(pid: u32) -> Option<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .and_then(|v| v.trim().parse().ok())
}

/// Context switches of every thread so far: how often the process went to
/// sleep and was woken. A process that is idle the way Ljós means it does
/// not wake at all.
#[must_use]
pub fn switches(pid: u32) -> u64 {
    thread_switches(pid).values().map(|(_, n)| n).sum()
}

/// [`switches`] by thread: its id, its name and its count.
#[must_use]
pub fn thread_switches(pid: u32) -> BTreeMap<u32, (String, u64)> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return BTreeMap::new();
    };
    tasks
        .flatten()
        .filter_map(|t| {
            let id = t.file_name().to_str()?.parse().ok()?;
            let status = fs::read_to_string(t.path().join("status")).ok()?;
            let name = fs::read_to_string(t.path().join("comm")).ok()?;
            let count = status
                .lines()
                .filter_map(|l| {
                    l.strip_prefix("voluntary_ctxt_switches:")
                        .or_else(|| l.strip_prefix("nonvoluntary_ctxt_switches:"))
                })
                .filter_map(|v| v.trim().parse::<u64>().ok())
                .sum::<u64>();
            Some((id, (name.trim().to_owned(), count)))
        })
        .collect()
}

/// The wakeups between two [`thread_switches`], by thread name: what woke
/// the process. A thread started between them counts from nothing.
#[must_use]
pub fn woken(
    before: &BTreeMap<u32, (String, u64)>,
    after: &BTreeMap<u32, (String, u64)>,
) -> BTreeMap<String, u64> {
    let mut by_name = BTreeMap::new();
    for (id, (name, n)) in after {
        let was = before.get(id).map_or(0, |(_, n)| *n);
        if *n > was {
            *by_name.entry(name.clone()).or_insert(0) += n - was;
        }
    }
    by_name
}

/// CPU time of every live thread so far, in nanoseconds, by thread id.
#[must_use]
pub fn thread_cpu_ns(pid: u32) -> BTreeMap<u32, u64> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return BTreeMap::new();
    };
    tasks
        .flatten()
        .filter_map(|t| {
            let id = t.file_name().to_str()?.parse().ok()?;
            let stat = fs::read_to_string(t.path().join("schedstat")).ok()?;
            Some((id, stat.split_whitespace().next()?.parse().ok()?))
        })
        .collect()
}

/// The CPU time, between two [`thread_cpu_ns`], of the threads still alive
/// at the end. What the process used beyond it went to threads that ended
/// in between, which [`woken`] cannot see either.
#[must_use]
pub fn live_cpu_ns(before: &BTreeMap<u32, u64>, after: &BTreeMap<u32, u64>) -> u64 {
    after
        .iter()
        .map(|(id, n)| n.saturating_sub(before.get(id).copied().unwrap_or(0)))
        .sum()
}

/// The CPU time of the threads alive at the end, between two
/// [`thread_cpu_ns`], in milliseconds by thread name (`names` from
/// [`thread_switches`]); threads of one name are added up.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn cpu_by_name(
    before: &BTreeMap<u32, u64>,
    after: &BTreeMap<u32, u64>,
    names: &BTreeMap<u32, (String, u64)>,
) -> BTreeMap<String, f64> {
    let mut by_name = BTreeMap::new();
    for (id, n) in after {
        let used = n.saturating_sub(before.get(id).copied().unwrap_or(0));
        if used == 0 {
            continue;
        }
        let name = names.get(id).map_or("?", |(name, _)| name.as_str());
        *by_name.entry(name.to_owned()).or_insert(0.0) += used as f64 / 1e6;
    }
    by_name
}

/// The one thread that used the most CPU time between two
/// [`thread_cpu_ns`]: what a runtime that splits its work over threads of
/// one name (a pool's workers take their parent's) has on a single core,
/// which [`cpu_by_name`] adds up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Busiest {
    pub name: String,
    pub cpu_ms: f64,
    /// How many live threads used CPU under that name.
    pub of_name: u32,
}

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn busiest_thread(
    before: &BTreeMap<u32, u64>,
    after: &BTreeMap<u32, u64>,
    names: &BTreeMap<u32, (String, u64)>,
) -> Option<Busiest> {
    let name = |id: &u32| names.get(id).map_or("?", |(name, _)| name.as_str());
    let used =
        |(id, n): (&u32, &u64)| (*id, n.saturating_sub(before.get(id).copied().unwrap_or(0)));
    let (id, ns) = after
        .iter()
        .map(used)
        .filter(|(_, ns)| *ns > 0)
        .max_by_key(|(_, ns)| *ns)?;
    let of_name = after
        .iter()
        .map(used)
        .filter(|(other, ns)| *ns > 0 && name(other) == name(&id))
        .count();
    Some(Busiest {
        name: name(&id).to_owned(),
        cpu_ms: ns as f64 / 1e6,
        of_name: u32::try_from(of_name).unwrap_or(u32::MAX),
    })
}

/// The program a process runs.
#[must_use]
pub fn exe(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// Whether `arg` is one of the arguments a process runs with.
#[must_use]
pub fn has_arg(pid: u32, arg: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    fs::read(format!("/proc/{pid}/cmdline"))
        .is_ok_and(|args| args.split(|&b| b == 0).any(|a| a == arg.as_bytes()))
}

/// A mapped file: its size on disk and how much of it is resident in the
/// process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapped {
    pub path: String,
    pub bytes: u64,
    pub rss: u64,
}

/// Every regular file the process has mapped, once each, with its size on
/// disk and its resident pages: what it needs installed to run, and what of
/// that it keeps in memory.
#[must_use]
pub fn mapped_files(pid: u32) -> Vec<Mapped> {
    let Ok(text) = fs::read_to_string(format!("/proc/{pid}/smaps")) else {
        return Vec::new();
    };
    let mut rss: BTreeMap<String, u64> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let first = line.split_whitespace().next().unwrap_or_default();
        if first.contains('-') && first.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            // A mapping's header: address perms offset dev inode path.
            let path = line.split_whitespace().nth(5).map(str::to_owned);
            current = path.filter(|p| p.starts_with('/'));
        } else if let (Some(path), Some(v)) = (&current, line.strip_prefix("Rss:")) {
            let kb: u64 = v.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
            *rss.entry(path.clone()).or_default() += kb * 1024;
        }
    }
    rss.into_iter()
        .filter_map(|(path, rss)| {
            let meta = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
            Some(Mapped {
                path,
                bytes: meta.len(),
                rss,
            })
        })
        .collect()
}

/// Which part of an install a mapped file belongs to.
#[must_use]
pub fn group(path: &str, runtime: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    if path == runtime {
        "runtime"
    } else if name.starts_with("libQt") || path.contains("/qt6/") || path.contains("/qt/") {
        "qt"
    } else if path.contains("/dri/")
        || [
            "libLLVM",
            "libgallium",
            "libEGL",
            "libGL",
            "libvulkan",
            "libgbm",
            "libdrm",
            "libglapi",
        ]
        .iter()
        .any(|p| name.starts_with(p))
        || path.contains("/fonts/")
    {
        if path.contains("/fonts/") {
            "fonts"
        } else {
            "graphics"
        }
    } else if path.contains("/locale/") || path.contains("/icons/") {
        "data"
    } else {
        "system"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_is_told_by_its_arguments() {
        let me = std::process::id();
        let first = std::env::args_os().next().unwrap();
        assert!(has_arg(me, &first));
        assert!(!has_arg(
            me,
            std::ffi::OsStr::new("--no-such-argument-here")
        ));
        // A part of an argument is not one.
        let part = &first.to_str().unwrap()[1..];
        assert!(!has_arg(me, std::ffi::OsStr::new(part)));
    }

    #[test]
    fn what_woke_is_told_by_thread_name() {
        let t = |list: &[(u32, &str, u64)]| -> BTreeMap<u32, (String, u64)> {
            list.iter()
                .map(|(id, name, n)| (*id, ((*name).to_owned(), *n)))
                .collect()
        };
        let before = t(&[
            (1, "main", 10),
            (2, "pool", 4),
            (3, "pool", 7),
            (4, "gone", 1),
        ]);
        let after = t(&[
            (1, "main", 10),
            (2, "pool", 6),
            (3, "pool", 8),
            (5, "new", 2),
        ]);
        let woke = woken(&before, &after);
        assert_eq!(woke.get("pool"), Some(&3));
        assert_eq!(woke.get("new"), Some(&2));
        assert!(!woke.contains_key("main"), "{woke:?}");
    }

    #[test]
    fn the_busiest_thread_is_one_thread_not_a_name() {
        let names: BTreeMap<u32, (String, u64)> = [(1, "app"), (2, "app"), (3, "app"), (4, "gpu")]
            .into_iter()
            .map(|(id, n)| (id, (n.to_owned(), 0)))
            .collect();
        let before = BTreeMap::from([(1, 0), (2, 0), (3, 0), (4, 0)]);
        let after = BTreeMap::from([
            (1, 4_000_000),
            (2, 3_000_000),
            (3, 3_000_000),
            (4, 6_000_000),
        ]);
        // By name, "app" used 10 ms to "gpu"'s 6; on one thread, "gpu" did most.
        assert_eq!(cpu_by_name(&before, &after, &names).get("app"), Some(&10.0));
        let b = busiest_thread(&before, &after, &names).unwrap();
        assert_eq!((b.name.as_str(), b.of_name), ("gpu", 1));
        assert!((b.cpu_ms - 6.0).abs() < 1e-9);
        let idle = busiest_thread(&before, &before, &names);
        assert_eq!(idle, None);
    }

    #[test]
    fn a_thread_that_ended_is_not_live_cpu() {
        let before = BTreeMap::from([(1, 100), (2, 50), (4, 30)]);
        let after = BTreeMap::from([(1, 160), (2, 50), (5, 20)]);
        assert_eq!(live_cpu_ns(&before, &after), 80);
        let me = std::process::id();
        let mine = thread_cpu_ns(me);
        assert!(mine.values().any(|n| *n > 0), "{mine:?}");
    }

    #[test]
    fn this_process_is_resident() {
        let me = std::process::id();
        let r = resident(me).unwrap();
        assert!(r.rss > 0 && r.pss > 0);
        assert!(threads(me).unwrap() >= 1);
        assert!(!mapped_files(me).is_empty());
    }

    #[test]
    fn files_are_grouped() {
        assert_eq!(group("/usr/lib/libQt6Quick.so.6.11.2", "/x"), "qt");
        assert_eq!(
            group("/usr/lib/qt6/qml/QtQuick/libqtquick2plugin.so", "/x"),
            "qt"
        );
        assert_eq!(group("/usr/lib/libLLVM.so.21", "/x"), "graphics");
        assert_eq!(group("/usr/lib/dri/swrast_dri.so", "/x"), "graphics");
        assert_eq!(group("/usr/lib/libc.so.6", "/x"), "system");
        assert_eq!(group("/x", "/x"), "runtime");
    }
}
