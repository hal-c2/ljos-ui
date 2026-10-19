//! Instructions a process's threads retire in user space, from the
//! processor's own counter (`perf_event_open`).
//!
//! CPU time on a loaded machine grows with what else runs: caches are
//! shared, the clock steps down, a thread waits for a core. The work a
//! thread does in instructions does not, so two runs of one build count
//! within a few per cent of each other where their CPU time can differ
//! twofold. A counter needs a processor that has one and a kernel that
//! lets a user read it (`perf_event_paranoid` at most 2); a virtual
//! machine often has neither, and the report then says why it has no
//! count.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read as _},
    os::fd::{FromRawFd as _, OwnedFd},
};

/// `struct perf_event_attr` up to `config2`; the kernel takes any size it
/// knows, and zero for the rest.
#[repr(C)]
#[derive(Default)]
struct Attr {
    kind: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
}

const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
const PERF_FORMAT_TOTAL_TIME_ENABLED: u64 = 1;
const PERF_FORMAT_TOTAL_TIME_RUNNING: u64 = 2;
/// `exclude_kernel` and `exclude_hv`: a user may count only its own code.
const USER_ONLY: u64 = (1 << 5) | (1 << 6);
const PERF_FLAG_FD_CLOEXEC: libc::c_ulong = 8;

/// A counter on each thread a process had when counting began.
#[derive(Debug)]
pub struct Instructions {
    open: Vec<(u32, fs::File)>,
}

impl Instructions {
    /// Starts counting on every thread of `pid`. A thread started later is
    /// not counted; a runtime starts its workers with its window.
    ///
    /// # Errors
    /// No thread could be counted: why, in words for the report.
    pub fn start(pid: u32) -> Result<Self, String> {
        let tasks = fs::read_dir(format!("/proc/{pid}/task")).map_err(|e| e.to_string())?;
        let mut open = Vec::new();
        let mut failed = None;
        for task in tasks.flatten() {
            let Some(tid) = task.file_name().to_str().and_then(|t| t.parse().ok()) else {
                continue;
            };
            match counter(tid) {
                Ok(fd) => open.push((tid, fs::File::from(fd))),
                Err(e) => failed = Some(e),
            }
        }
        if open.is_empty() {
            return Err(match failed {
                Some(e) => format!("the processor's counter cannot be read: {e}"),
                None => "the process has no threads".to_owned(),
            });
        }
        Ok(Instructions { open })
    }

    /// What each thread retired since [`Instructions::start`], added up by
    /// thread name (`names` from [`crate::procfs::thread_switches`]). A
    /// counter the kernel shared out with others is scaled to the whole
    /// time.
    #[must_use]
    pub fn finish(self, names: &BTreeMap<u32, (String, u64)>) -> BTreeMap<String, u64> {
        let mut by_name = BTreeMap::new();
        for (tid, mut file) in self.open {
            let mut buf = [0u8; 24];
            if file.read_exact(&mut buf).is_err() {
                continue;
            }
            let word = |i: usize| u64::from_ne_bytes(buf[i * 8..i * 8 + 8].try_into().unwrap());
            let (count, enabled, running) = (word(0), word(1), word(2));
            let count = if running > 0 && running < enabled {
                u64::try_from(u128::from(count) * u128::from(enabled) / u128::from(running))
                    .unwrap_or(u64::MAX)
            } else {
                count
            };
            if count == 0 {
                continue;
            }
            let name = names.get(&tid).map_or("?", |(name, _)| name.as_str());
            *by_name.entry(name.to_owned()).or_insert(0) += count;
        }
        by_name
    }
}

fn counter(tid: u32) -> io::Result<OwnedFd> {
    let attr = Attr {
        kind: PERF_TYPE_HARDWARE,
        size: u32::try_from(size_of::<Attr>()).expect("small"),
        config: PERF_COUNT_HW_INSTRUCTIONS,
        read_format: PERF_FORMAT_TOTAL_TIME_ENABLED | PERF_FORMAT_TOTAL_TIME_RUNNING,
        flags: USER_ONLY,
        ..Attr::default()
    };
    let tid =
        libc::pid_t::try_from(tid).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `attr` is a valid perf_event_attr of the size it says and
    // outlives the call; the result is a new descriptor or -1.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_perf_event_open,
            &raw const attr,
            tid,
            -1 as libc::c_int,
            -1 as libc::c_int,
            PERF_FLAG_FD_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = libc::c_int::try_from(fd).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    // SAFETY: the kernel just gave this descriptor, and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same work counted twice: where the processor counts at all, it
    /// counts it within a per cent, whatever else the machine is doing.
    #[test]
    fn the_same_work_counts_the_same() {
        let work = || {
            let mut x = 1u64;
            for i in 0..2_000_000u64 {
                x = std::hint::black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i));
            }
            x
        };
        let mut counts = Vec::new();
        for _ in 0..2 {
            // A thread of its own, counted from its start, so the test
            // harness's other threads are not in it.
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel::<u32>();
            let worker = std::thread::spawn(move || {
                // SAFETY: gettid has no preconditions.
                let tid = u32::try_from(unsafe { libc::gettid() }).unwrap();
                ready_tx.send(tid).unwrap();
                rx.recv().unwrap();
                work()
            });
            let tid = ready_rx.recv().unwrap();
            let fd = match counter(tid) {
                Ok(fd) => fd,
                Err(e) => {
                    eprintln!("no instruction counter here ({e}); nothing to check");
                    tx.send(()).unwrap();
                    worker.join().unwrap();
                    return;
                }
            };
            let counted = Instructions {
                open: vec![(tid, fs::File::from(fd))],
            };
            tx.send(()).unwrap();
            worker.join().unwrap();
            let names = BTreeMap::from([(tid, ("worker".to_owned(), 0))]);
            counts.push(counted.finish(&names)["worker"]);
        }
        let (a, b) = (counts[0] as f64, counts[1] as f64);
        assert!(a > 2_000_000.0, "{a} instructions for 2 000 000 turns");
        assert!((a - b).abs() / a < 0.01, "{a} and {b}");
    }
}
