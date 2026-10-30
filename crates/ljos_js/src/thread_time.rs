//! The time the calling thread has had the processor, as an [`Instant`] so
//! that it reads like the clocks a test sets ([`crate::Timing::Thread`]).
//!
//! Windows counts it in clock ticks of about 15 ms: a difference between
//! two readings may be that much short, which only excuses that much more.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub(crate) fn now() -> Option<Instant> {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    Some(*EPOCH.get_or_init(Instant::now) + used()?)
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
fn used() -> Option<Duration> {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `t` is valid for the write, and the clock needs nothing else.
    let r = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut t) };
    (r == 0).then(|| Duration::new(t.tv_sec as u64, t.tv_nsec as u32))
}

#[cfg(windows)]
fn used() -> Option<Duration> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: all four are valid for the write, and the pseudo handle of the
    // current thread needs no closing.
    let ok = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    };
    let hundred_ns = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    (ok != 0).then(|| Duration::from_nanos((hundred_ns(kernel) + hundred_ns(user)) * 100))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
fn used() -> Option<Duration> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        windows
    ))]
    #[test]
    fn sleeping_uses_no_time_and_spinning_does() {
        let before = now().expect("this target has a thread clock");
        std::thread::sleep(Duration::from_millis(100));
        let slept = now().unwrap() - before;
        assert!(slept < Duration::from_millis(50), "{slept:?}");
        let before = now().unwrap();
        let wall = Instant::now();
        while wall.elapsed() < Duration::from_millis(100) {
            std::hint::spin_loop();
        }
        let spun = now().unwrap() - before;
        assert!(spun > Duration::ZERO, "{spun:?}");
    }
}
