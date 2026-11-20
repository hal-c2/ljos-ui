//! A cgroup's accounting, read from its files (cgroup v2).

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The cgroup a process is in, under `root` (normally `/sys/fs/cgroup`).
///
/// # Errors
/// The process is gone or its cgroup line is not a v2 one.
pub fn of_pid(root: &Path, pid: u32) -> std::io::Result<PathBuf> {
    let text = fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(|path| root.join(path.trim_start_matches('/')))
        .ok_or_else(|| std::io::Error::other(format!("process {pid} has no cgroup v2 path")))
}

/// Whether `path` is on a cgroup v2 filesystem. A directory that only
/// looks like one (a test's stand-in) is not.
#[must_use]
pub fn is_cgroup2(path: &Path) -> bool {
    const CGROUP2_SUPER_MAGIC: i64 = 0x6367_7270;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    // SAFETY: statfs(2) fills the zeroed struct for a NUL-terminated path
    // that lives across the call.
    unsafe {
        let mut st: libc::statfs = std::mem::zeroed();
        // `f_type`'s width differs between targets.
        #[allow(clippy::unnecessary_cast, clippy::cast_lossless, trivial_numeric_casts)]
        let ok =
            libc::statfs(c.as_ptr(), &raw mut st) == 0 && st.f_type as i64 == CGROUP2_SUPER_MAGIC;
        ok
    }
}

/// A cgroup directory.
#[derive(Clone, Debug)]
pub struct Cgroup {
    pub dir: PathBuf,
}

impl Cgroup {
    fn read(&self, file: &str) -> Option<String> {
        fs::read_to_string(self.dir.join(file)).ok()
    }

    /// A file holding one number; `max` is `None`.
    fn number(&self, file: &str) -> Option<u64> {
        self.read(file)?.trim().parse().ok()
    }

    /// A `key value` line of a flat-keyed file.
    fn keyed(&self, file: &str, key: &str) -> Option<u64> {
        keyed(&self.read(file)?, key)
    }

    /// Writes one of the cgroup's files.
    ///
    /// # Errors
    /// The file cannot be written: it is not there, the controller is not
    /// enabled, or the value is refused.
    pub fn write(&self, file: &str, value: &str) -> std::io::Result<()> {
        fs::write(self.dir.join(file), value)
    }

    /// The controllers its parent hands it (`cgroup.controllers`).
    #[must_use]
    pub fn controllers(&self) -> Vec<String> {
        self.words("cgroup.controllers")
    }

    /// The controllers it hands its children (`cgroup.subtree_control`).
    #[must_use]
    pub fn subtree_control(&self) -> Vec<String> {
        self.words("cgroup.subtree_control")
    }

    fn words(&self, file: &str) -> Vec<String> {
        self.read(file)
            .map(|s| s.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn memory_max(&self) -> Option<String> {
        self.read("memory.max").map(|s| s.trim().to_owned())
    }

    #[must_use]
    pub fn swap_max(&self) -> Option<String> {
        self.read("memory.swap.max").map(|s| s.trim().to_owned())
    }

    #[must_use]
    pub fn cpu_max(&self) -> Option<String> {
        self.read("cpu.max").map(|s| s.trim().to_owned())
    }

    #[must_use]
    pub fn cpus(&self) -> Option<String> {
        self.read("cpuset.cpus.effective")
            .map(|s| s.trim().to_owned())
    }

    /// The pids in the cgroup itself, not below it.
    #[must_use]
    pub fn procs(&self) -> Option<Vec<u32>> {
        Some(
            self.read("cgroup.procs")?
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect(),
        )
    }

    #[must_use]
    pub fn peak(&self) -> Option<u64> {
        self.number("memory.peak")
    }

    #[must_use]
    pub fn current(&self) -> Option<u64> {
        self.number("memory.current")
    }

    /// How many processes the kernel killed here for want of memory.
    #[must_use]
    pub fn oom_kills(&self) -> Option<u64> {
        self.keyed("memory.events", "oom_kill")
    }

    /// CPU time used, in microseconds.
    #[must_use]
    pub fn cpu_usec(&self) -> Option<u64> {
        self.keyed("cpu.stat", "usage_usec")
    }

    /// Microseconds the quota held the cgroup back.
    #[must_use]
    pub fn throttled_usec(&self) -> Option<u64> {
        self.keyed("cpu.stat", "throttled_usec")
    }

    /// A field of `memory.stat` (`anon`, `file`, `kernel`, …), in bytes.
    #[must_use]
    pub fn stat(&self, key: &str) -> Option<u64> {
        self.keyed("memory.stat", key)
    }
}

fn keyed(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once(' ')?;
        (k == key).then(|| v.trim().parse().ok()).flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_keyed_files() {
        let text = "oom 2\noom_kill 1\noom_group_kill 0\n";
        assert_eq!(keyed(text, "oom_kill"), Some(1));
        assert_eq!(keyed(text, "oom"), Some(2));
        assert_eq!(keyed(text, "max"), None);
    }

    #[test]
    fn a_cgroup_is_read_from_its_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("memory.peak"), "4096\n").unwrap();
        fs::write(dir.path().join("cpu.stat"), "usage_usec 77\nuser_usec 70\n").unwrap();
        fs::write(dir.path().join("cgroup.procs"), "12\n13\n").unwrap();
        let cg = Cgroup {
            dir: dir.path().to_owned(),
        };
        assert_eq!(cg.peak(), Some(4096));
        assert_eq!(cg.cpu_usec(), Some(77));
        assert_eq!(cg.procs(), Some(vec![12, 13]));
        assert_eq!(cg.current(), None);
    }
}
