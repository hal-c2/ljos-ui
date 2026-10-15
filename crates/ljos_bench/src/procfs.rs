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
    let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) else {
        return 0;
    };
    tasks
        .flatten()
        .filter_map(|t| fs::read_to_string(t.path().join("status")).ok())
        .map(|text| {
            text.lines()
                .filter_map(|l| {
                    l.strip_prefix("voluntary_ctxt_switches:")
                        .or_else(|| l.strip_prefix("nonvoluntary_ctxt_switches:"))
                })
                .filter_map(|v| v.trim().parse::<u64>().ok())
                .sum::<u64>()
        })
        .sum()
}

/// The program a process runs.
#[must_use]
pub fn exe(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
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
