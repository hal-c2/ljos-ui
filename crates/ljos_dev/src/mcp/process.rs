//! The process as the platform tells it: its threads, its memory and the
//! CPU time it used, for `app_processes`, the memory tool and
//! `app_metrics`. One implementation a platform, each on the crate that
//! reads it there: procfs on Linux, libproc (with Mach for what it does
//! not wrap) on macOS, Win32 on Windows. What a platform does not tell is
//! None, never 0, so an answer can say it was not measured.
//!
//! Each call asks the kernel for counts it keeps anyway; nothing is
//! sampled in between, so the process costs nothing until asked.

#[cfg(target_os = "linux")]
#[path = "process/linux.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "process/macos.rs"]
mod platform;

#[cfg(windows)]
#[path = "process/windows.rs"]
mod platform;

/// A platform Ljós reads nothing of the process on.
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::{Process, Resident};

    pub(super) const NAME_KEPT: usize = usize::MAX;

    pub(super) fn current_tid() -> Option<u64> {
        None
    }

    pub(super) fn process() -> Process {
        Process::default()
    }

    pub(super) fn resident() -> Option<Resident> {
        None
    }
}

#[cfg(target_os = "macos")]
pub(crate) use platform::regions;

/// The bytes of a thread's name the kernel keeps: a longer one is cut.
pub(crate) const NAME_KEPT: usize = platform::NAME_KEPT;

/// One OS thread, in the letters `/proc` uses for its state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Thread {
    pub(crate) tid: u64,
    pub(crate) name: String,
    /// `R`, `S`, `D`, ... None where the platform does not tell it.
    pub(crate) state: Option<char>,
    pub(crate) work_us: u64,
    /// Times it went to sleep of its own accord, and times it was made to
    /// give way. None where the platform does not count them.
    pub(crate) wakeups: Option<u64>,
    pub(crate) preempted: Option<u64>,
}

/// Bytes of the process: its address space, what is resident, of that
/// what is backed by files or shared (libraries, the binary, shared
/// buffers), which other processes may hold too, and what is its own.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resident {
    pub(crate) virt: u64,
    pub(crate) rss: u64,
    pub(crate) shared: u64,
    own: u64,
}

impl Resident {
    /// The process's memory now, or None where the platform does not tell.
    pub(crate) fn now() -> Option<Resident> {
        platform::resident()
    }

    /// The process's own: heap, stacks, anonymous maps. On macOS the
    /// footprint the system charges it, which counts what it holds
    /// compressed too.
    pub(crate) fn own(&self) -> u64 {
        self.own
    }
}

/// The process at one moment, as far as the platform tells.
#[derive(Clone, Debug, Default)]
pub(crate) struct Process {
    /// Every thread, by id; None where the platform lists none.
    pub(crate) threads: Option<Vec<Thread>>,
    pub(crate) resident: Option<Resident>,
    /// CPU time of every thread there was, the ended ones too.
    pub(crate) cpu_us: Option<u64>,
    /// Wakeups of the whole process, where the platform counts them for
    /// it and not for each thread.
    pub(crate) wakeups: Option<u64>,
}

impl Process {
    pub(crate) fn now() -> Process {
        platform::process()
    }
}

/// The calling thread, as [`Process::threads`] numbers them.
pub(crate) fn current_tid() -> Option<u64> {
    platform::current_tid()
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod tests {
    use super::*;

    /// The listing of `tid`. A thread that ends meanwhile (another
    /// test's) can make one listing skip a living thread, as the kernel
    /// walks a task directory by position.
    fn listed(tid: u64) -> Thread {
        (0..20)
            .find_map(|_| Process::now().threads?.into_iter().find(|t| t.tid == tid))
            .unwrap()
    }

    #[test]
    fn this_thread_is_listed_by_its_name() {
        let me = current_tid().unwrap();
        assert!(listed(me).work_us > 0);
        let (tid, it) = std::thread::Builder::new()
            .name("ljos-proc-test".into())
            .spawn(|| {
                let tid = current_tid().unwrap();
                (tid, listed(tid))
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(it.tid, tid);
        assert_eq!(it.name, "ljos-proc-test");
        // It is running: it is the one asking.
        assert!(matches!(it.state, Some('R') | None), "{:?}", it.state);
    }

    #[test]
    fn the_process_is_measured() {
        let p = Process::now();
        assert!(!p.threads.unwrap().is_empty());
        let r = p.resident.unwrap();
        assert!(r.rss > 0 && r.own > 0);
        assert!(r.virt >= r.rss);
        assert_eq!(r.shared, r.rss.saturating_sub(r.own));
        // Counted in ticks of 10 ms on Linux, so a test that starts
        // first may have used none yet.
        assert!(p.cpu_us.is_some());
    }

    /// The CPU time of a thread that spins is counted, in microseconds:
    /// a count in other units runs faster than the wall clock, or the
    /// process's slower than its thread's. On a busy machine the spin
    /// gets less of the CPU than the wall clock passes, so it spins until
    /// its own count has grown.
    #[test]
    fn a_busy_thread_counts_its_cpu_time() {
        const SPUN: u64 = 50_000;
        let wall = std::time::Instant::now();
        let (spun, took, process) = std::thread::spawn(|| {
            let tid = current_tid().unwrap();
            let process = Process::now().cpu_us.unwrap();
            let before = listed(tid).work_us;
            let start = std::time::Instant::now();
            let mut spun = 0;
            while spun < SPUN && start.elapsed().as_secs() < 30 {
                let lap = std::time::Instant::now();
                let mut x = 0u64;
                while lap.elapsed().as_millis() < 5 {
                    x = std::hint::black_box(x.wrapping_add(1));
                }
                spun = listed(tid).work_us - before;
            }
            (spun, start.elapsed(), process)
        })
        .join()
        .unwrap();
        assert!(spun >= SPUN, "{spun} µs in {took:?}");
        // No thread runs faster than the wall clock (a tick of slack).
        assert!(
            spun <= took.as_micros() as u64 + 10_000,
            "{spun} µs in {took:?}"
        );
        // The process counts every thread's time, this one's included,
        // and its threads together run no faster than every CPU.
        let used = Process::now().cpu_us.unwrap() - process;
        assert!(
            used + 10_000 >= spun,
            "{used} µs of the process, {spun} of the thread"
        );
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
        let most = wall.elapsed().as_micros() as u64 * cpus + 50_000;
        assert!(used <= most, "{used} µs of the process in {most} µs");
    }

    #[test]
    fn the_process_counts_at_least_its_living_threads() {
        let p = Process::now();
        let threads: u64 = p.threads.unwrap().iter().map(|t| t.work_us).sum();
        // Read after the threads: it can only have grown since.
        let cpu = Process::now().cpu_us.unwrap();
        // The process's own count may be in coarser ticks than its
        // threads'.
        assert!(cpu + 50_000 >= threads, "{cpu} < {threads}");
    }

    #[test]
    fn touched_pages_are_resident_and_the_process_own() {
        let r = Resident::now().unwrap();
        let block = std::hint::black_box(vec![1u8; 16 << 20]);
        let after = Resident::now().unwrap();
        assert!(after.own >= r.own + (8 << 20), "{r:?} {after:?}");
        assert!(after.rss >= r.rss + (8 << 20), "{r:?} {after:?}");
        drop(block);
    }
}
