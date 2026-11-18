//! Where a run reads what the application used: its own cgroup, or, where
//! it shares one or has none, its processes.

use crate::{cgroup::Cgroup, throttle::Throttle, tree};

#[derive(Debug)]
pub(super) enum Accounting {
    /// A cgroup that holds the application alone.
    Own(Cgroup),
    /// The process `pid` and those below it.
    Tree {
        pid: u32,
        /// The cgroup that holds the memory limit, whose kills for want of
        /// memory are counted from `oom0`, and the one that holds the CPU
        /// quota, whose time held back is the application's too.
        memory: Option<Cgroup>,
        cpu: Option<Cgroup>,
        oom0: u64,
        /// The quota the throttle holds once the gate opens, and the
        /// throttle then.
        quota: Option<(u64, u64)>,
        throttle: Option<Throttle>,
    },
}

impl Accounting {
    pub(super) fn tree(
        pid: u32,
        memory: Option<Cgroup>,
        cpu: Option<Cgroup>,
        quota: Option<(u64, u64)>,
    ) -> Self {
        let oom0 = memory.as_ref().and_then(Cgroup::oom_kills).unwrap_or(0);
        Self::Tree {
            pid,
            memory,
            cpu,
            oom0,
            quota,
            throttle: None,
        }
    }

    /// Whether others share what is read: a kill for want of memory may
    /// have been of another process.
    pub(super) fn shared(&self) -> bool {
        matches!(self, Self::Tree { .. })
    }

    /// Starts holding the quota, when it is the bench's to hold.
    pub(super) fn start(&mut self) {
        if let Self::Tree {
            pid,
            quota: Some((q, p)),
            throttle,
            ..
        } = self
        {
            *throttle = Some(Throttle::start(*pid, *q, *p));
        }
    }

    /// Stops holding it, so a stopped application can be asked to end.
    pub(super) fn end(&mut self) {
        if let Self::Tree { throttle, .. } = self {
            *throttle = None;
        }
    }

    fn memory(&self) -> Option<tree::Memory> {
        match self {
            Self::Own(_) => None,
            Self::Tree { pid, .. } => Some(tree::memory(&tree::of(*pid))),
        }
    }

    pub(super) fn cpu_usec(&self) -> Option<u64> {
        match self {
            Self::Own(c) => c.cpu_usec(),
            Self::Tree { pid, .. } => tree::cpu_usec(&tree::of(*pid)),
        }
    }

    pub(super) fn throttled_usec(&self) -> Option<u64> {
        match self {
            Self::Own(c) => c.throttled_usec(),
            Self::Tree {
                throttle: Some(t), ..
            } => Some(t.throttled_usec()),
            Self::Tree { cpu, .. } => cpu.as_ref().and_then(Cgroup::throttled_usec),
        }
    }

    /// The most memory held so far; from `/proc`, the most of now's total
    /// and any one process's own high-water mark.
    pub(super) fn peak(&self) -> Option<u64> {
        match self {
            Self::Own(c) => c.peak(),
            Self::Tree { .. } => self.memory().map(|m| m.rss.max(m.hwm)),
        }
    }

    pub(super) fn current(&self) -> Option<u64> {
        match self {
            Self::Own(c) => c.current(),
            Self::Tree { .. } => self.memory().map(|m| m.rss),
        }
    }

    /// A field of `memory.stat`; from `/proc`, only `anon` and `file`.
    pub(super) fn stat(&self, key: &str) -> Option<u64> {
        match self {
            Self::Own(c) => c.stat(key),
            Self::Tree { .. } => {
                let m = self.memory()?;
                match key {
                    "anon" => Some(m.anon),
                    "file" => Some(m.file),
                    _ => None,
                }
            }
        }
    }

    pub(super) fn oom_kills(&self) -> Option<u64> {
        match self {
            Self::Own(c) => c.oom_kills(),
            Self::Tree { memory, oom0, .. } => memory
                .as_ref()
                .and_then(Cgroup::oom_kills)
                .map(|n| n.saturating_sub(*oom0)),
        }
    }
}
