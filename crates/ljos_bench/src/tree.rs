//! An application's processes, read from `/proc`: what is accounted where
//! the application has no cgroup of its own (it shares the container's, or
//! is held by limits on the process). A cgroup counts everything charged
//! to it; this counts only what its live processes hold, so page cache
//! and kernel memory are missing, and the CPU of a child that ended is
//! counted only once it was waited for.

use std::{collections::BTreeSet, fs};

/// The process and every process below it, the process first.
#[must_use]
pub fn of(pid: u32) -> Vec<u32> {
    let mut out = vec![pid];
    let mut seen = BTreeSet::from([pid]);
    let mut i = 0;
    while i < out.len() {
        for child in children(out[i]) {
            if seen.insert(child) {
                out.push(child);
            }
        }
        i += 1;
    }
    out
}

/// The children of `pid`, from each of its threads' `children` file, or
/// by scanning `/proc` where the kernel has no such files.
fn children(pid: u32) -> Vec<u32> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut any = false;
    for t in tasks.flatten() {
        if let Ok(text) = fs::read_to_string(t.path().join("children")) {
            any = true;
            out.extend(
                text.split_whitespace()
                    .filter_map(|n| n.parse::<u32>().ok()),
            );
        }
    }
    if any {
        return out;
    }
    let Ok(all) = fs::read_dir("/proc") else {
        return out;
    };
    all.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|&p| stat(p).is_some_and(|s| s.ppid == pid))
        .collect()
}

/// What `/proc/<pid>/stat` says after the command name.
struct Stat {
    ppid: u32,
    /// Its own CPU and that of the children it waited for, in clock ticks.
    ticks: u64,
}

fn stat(pid: u32) -> Option<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The name is in parentheses and may hold anything, spaces too.
    let rest = &text[text.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // Fields 3 on in proc(5), so ppid is the second here and utime,
    // stime, cutime and cstime the 12th to 15th.
    let n = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok());
    Some(Stat {
        ppid: u32::try_from(n(1)?).ok()?,
        ticks: n(11)? + n(12)? + n(13)? + n(14)?,
    })
}

/// Memory of a set of processes from their `status`, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Resident: anonymous, file and shared memory.
    pub rss: u64,
    pub anon: u64,
    pub file: u64,
    /// The most any one of them was ever resident in (`VmHWM`).
    pub hwm: u64,
}

/// The memory of the processes `pids`, from `/proc/<pid>/status`.
#[must_use]
pub fn memory(pids: &[u32]) -> Memory {
    let mut out = Memory::default();
    for pid in pids {
        let Ok(text) = fs::read_to_string(format!("/proc/{pid}/status")) else {
            continue;
        };
        let m = status_memory(&text);
        out.rss += m.rss;
        out.anon += m.anon;
        out.file += m.file;
        out.hwm = out.hwm.max(m.hwm);
    }
    out
}

fn status_memory(text: &str) -> Memory {
    let kb = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map_or(0, |v| v * 1024)
    };
    Memory {
        rss: kb("VmRSS"),
        anon: kb("RssAnon"),
        file: kb("RssFile"),
        hwm: kb("VmHWM"),
    }
}

/// The CPU the processes `pids` used, in microseconds, at the clock tick's
/// grain; a child's counts once its parent has waited for it.
#[must_use]
pub fn cpu_usec(pids: &[u32]) -> Option<u64> {
    // SAFETY: sysconf(3) has no preconditions.
    let hz = u64::try_from(unsafe { libc::sysconf(libc::_SC_CLK_TCK) })
        .ok()
        .filter(|&h| h > 0)?;
    let ticks: u64 = pids.iter().filter_map(|&p| stat(p)).map(|s| s.ticks).sum();
    Some(ticks * 1_000_000 / hz)
}

/// The CPU every live thread of the processes `pids` has run for, in
/// nanoseconds (`schedstat`): fine enough to hold a quota a period of
/// milliseconds at a time.
#[must_use]
pub fn cpu_ns(pids: &[u32]) -> u64 {
    pids.iter()
        .map(|&p| crate::procfs::thread_cpu_ns(p).values().sum::<u64>())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_and_what_it_started_are_its_tree() {
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut tree = of(pid);
        while tree.len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            tree = of(pid);
        }
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(tree[0], pid);
        assert_eq!(tree.len(), 2, "{tree:?}");
        assert!(cpu_usec(&tree).is_some());
    }

    #[test]
    fn memory_is_read_from_status() {
        let text = "Name:\tx\nVmHWM:\t  2048 kB\nVmRSS:\t  1536 kB\nRssAnon:\t  1024 kB\nRssFile:\t   512 kB\n";
        assert_eq!(
            status_memory(text),
            Memory {
                rss: 1536 << 10,
                anon: 1024 << 10,
                file: 512 << 10,
                hwm: 2048 << 10,
            }
        );
    }

    #[test]
    fn this_process_has_memory() {
        let m = memory(&[std::process::id()]);
        assert!(m.rss > 0 && m.hwm >= m.rss);
    }
}
