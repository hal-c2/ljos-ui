//! A CPU quota held without a cgroup: the application's process group is
//! stopped when it has used its share and continued when the share has
//! grown back. It holds the average a cgroup's quota holds, not the grain:
//! the application runs at full speed for up to one look before it is
//! stopped, then waits out what it overspent.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// How often the application's CPU is looked at.
pub const EVERY: Duration = Duration::from_millis(10);

/// The bucket of CPU the application may still use, in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bucket {
    /// CPU nanoseconds a nanosecond of time gives.
    rate: f64,
    /// The most that can be saved up: one period's quota, as a cgroup
    /// lets nothing carry over from one period to the next.
    cap: f64,
    left: f64,
    stopped: bool,
}

/// What to do after a look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Stop,
    Continue,
}

impl Bucket {
    /// `quota` microseconds of CPU in each `period`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn new(quota: u64, period: u64) -> Self {
        let cap = quota as f64 * 1000.0;
        Self {
            rate: quota as f64 / period.max(1) as f64,
            cap,
            left: cap,
            stopped: false,
        }
    }

    /// `elapsed` nanoseconds went by, in which the application used `used`.
    #[allow(clippy::cast_precision_loss)]
    pub fn step(&mut self, elapsed: u64, used: u64) -> Action {
        self.left = (self.left + elapsed as f64 * self.rate - used as f64).min(self.cap);
        match (self.stopped, self.left < 0.0) {
            (false, true) => {
                self.stopped = true;
                Action::Stop
            }
            (true, false) => {
                self.stopped = false;
                Action::Continue
            }
            _ => Action::Nothing,
        }
    }
}

/// The thread that holds a process group to a quota. Dropped, it stops
/// and leaves the group running.
#[derive(Debug)]
pub struct Throttle {
    done: Arc<AtomicBool>,
    stopped_ns: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

impl Throttle {
    /// Holds the group `pgid` (its leader's pid) to `quota` microseconds of
    /// CPU in each `period`.
    #[must_use]
    pub fn start(pgid: u32, quota: u64, period: u64) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let stopped_ns = Arc::new(AtomicU64::new(0));
        let (d, s) = (done.clone(), stopped_ns.clone());
        let thread = thread::Builder::new()
            .name("bench-throttle".to_owned())
            .spawn(move || hold(pgid, quota, period, &d, &s))
            .ok();
        Self {
            done,
            stopped_ns,
            thread,
        }
    }

    /// How long the group has been held stopped, in microseconds.
    #[must_use]
    pub fn throttled_usec(&self) -> u64 {
        self.stopped_ns.load(Ordering::Relaxed) / 1000
    }
}

impl Drop for Throttle {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn signal_group(pgid: u32, sig: i32) -> bool {
    let Ok(pgid) = i32::try_from(pgid) else {
        return false;
    };
    // SAFETY: kill(2) on a process group the bench started; a negative pid
    // names the group.
    unsafe { libc::kill(-pgid, sig) == 0 }
}

fn hold(pgid: u32, quota: u64, period: u64, done: &AtomicBool, stopped_ns: &AtomicU64) {
    let mut bucket = Bucket::new(quota, period);
    let mut last = Instant::now();
    let mut cpu = crate::tree::cpu_ns(&crate::tree::of(pgid));
    let mut stopped_at: Option<Instant> = None;
    let count = |since: Instant| {
        let ns = u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX);
        stopped_ns.fetch_add(ns, Ordering::Relaxed);
    };
    while !done.load(Ordering::Relaxed) {
        thread::sleep(EVERY);
        let now = Instant::now();
        let elapsed = u64::try_from(now.duration_since(last).as_nanos()).unwrap_or(u64::MAX);
        last = now;
        let pids = crate::tree::of(pgid);
        let used_now = crate::tree::cpu_ns(&pids);
        // A thread that ended takes its time with it; that is not negative use.
        let used = used_now.saturating_sub(cpu);
        cpu = used_now;
        match bucket.step(elapsed, used) {
            Action::Stop => {
                if !signal_group(pgid, libc::SIGSTOP) {
                    break;
                }
                stopped_at = Some(now);
            }
            Action::Continue => {
                signal_group(pgid, libc::SIGCONT);
                if let Some(at) = stopped_at.take() {
                    count(at);
                }
            }
            Action::Nothing => {}
        }
    }
    if let Some(at) = stopped_at {
        signal_group(pgid, libc::SIGCONT);
        count(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_over_its_share_is_stopped_until_the_share_grows_back() {
        // 20 % of one CPU over a 10 ms period: 2 ms of CPU a period.
        let mut b = Bucket::new(2000, 10_000);
        // 10 ms of time, all of it on the CPU: with the 2 ms saved and the
        // 2 ms it gave, 6 ms over.
        assert_eq!(b.step(10_000_000, 10_000_000), Action::Stop);
        // Stopped, it uses nothing; 6 ms over at 0.2 is 30 ms to wait.
        assert_eq!(b.step(20_000_000, 0), Action::Nothing);
        assert_eq!(b.step(10_000_000, 0), Action::Continue);
    }

    #[test]
    fn idle_time_saves_up_one_period_only() {
        let mut b = Bucket::new(2000, 10_000);
        assert_eq!(b.step(1_000_000_000, 0), Action::Nothing);
        // A second of idling still allows only 2 ms more than the 2 ms the
        // next 10 ms give.
        assert_eq!(b.step(10_000_000, 4_000_000), Action::Nothing);
        assert_eq!(b.step(10_000_000, 3_000_000), Action::Stop);
    }

    #[test]
    fn a_busy_process_is_held_to_its_share() {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id();
        let start = crate::tree::cpu_ns(&[pid]);
        let t0 = Instant::now();
        let throttle = Throttle::start(pid, 2000, 10_000);
        thread::sleep(Duration::from_millis(1000));
        let throttled = throttle.throttled_usec();
        drop(throttle);
        let used = crate::tree::cpu_ns(&[pid]) - start;
        let _ = child.kill();
        let _ = child.wait();
        #[allow(clippy::cast_precision_loss)]
        let share = used as f64 / t0.elapsed().as_nanos() as f64;
        // 20 %, and what one look lets through; unheld it would be all of one.
        assert!(share < 0.35, "used {share:.2} of a CPU");
        assert!(throttled > 0);
    }
}
