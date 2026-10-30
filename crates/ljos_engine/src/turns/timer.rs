#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use std::io;
    use std::sync::Arc;
    use std::time::Duration;

    pub enum Wake {
        Changed,
        Fired(usize),
    }

    struct Queue {
        changed: libc::c_int,
    }

    impl Queue {
        fn new() -> io::Result<Queue> {
            // SAFETY: the flags are valid and the returned descriptor is owned by Queue.
            let changed = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
            if changed < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(Queue { changed })
            }
        }

        fn changed(&self) {
            let value = 1_u64;
            // SAFETY: value is a valid u64 buffer and the descriptor stays open for self's lifetime.
            let written = unsafe {
                libc::write(
                    self.changed,
                    (&value as *const u64).cast(),
                    size_of::<u64>(),
                )
            };
            assert_eq!(
                written,
                size_of::<u64>() as isize,
                "eventfd write failed: {}",
                io::Error::last_os_error()
            );
        }
    }

    impl Drop for Queue {
        fn drop(&mut self) {
            // SAFETY: Queue owns this descriptor and no other owner closes it.
            unsafe { libc::close(self.changed) };
        }
    }

    struct Inner {
        queue: Arc<Queue>,
        fd: libc::c_int,
    }

    impl Inner {
        fn set(&self, duration: Duration) {
            let value = libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                it_value: libc::timespec {
                    tv_sec: libc::time_t::try_from(duration.as_secs()).unwrap_or(libc::time_t::MAX),
                    tv_nsec: duration.subsec_nanos() as libc::c_long,
                },
            };
            // SAFETY: fd is a live timerfd and value points to a valid immutable timespec.
            let result = unsafe { libc::timerfd_settime(self.fd, 0, &value, std::ptr::null_mut()) };
            assert_eq!(
                result,
                0,
                "timerfd_settime failed: {}",
                io::Error::last_os_error()
            );
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // SAFETY: Inner owns this descriptor and no other owner closes it.
            unsafe { libc::close(self.fd) };
        }
    }

    #[derive(Clone)]
    pub struct Timer(Arc<Inner>);

    impl Timer {
        pub fn arm(&self, duration: Duration) {
            self.0.set(duration.max(Duration::from_nanos(1)));
        }

        pub fn disarm(&self) {
            self.0.set(Duration::ZERO);
        }

        pub fn remove(&self) {
            self.disarm();
            self.0.queue.changed();
        }
    }

    pub struct Waiter {
        queue: Arc<Queue>,
    }

    impl Waiter {
        pub fn new() -> Waiter {
            Waiter {
                queue: Arc::new(Queue::new().expect("the watchdog's eventfd opens")),
            }
        }

        pub fn timer(&self) -> Timer {
            // SAFETY: CLOCK_MONOTONIC and the flags are valid for timerfd_create.
            let fd = unsafe {
                libc::timerfd_create(
                    libc::CLOCK_MONOTONIC,
                    libc::TFD_CLOEXEC | libc::TFD_NONBLOCK,
                )
            };
            assert!(
                fd >= 0,
                "timerfd_create failed: {}",
                io::Error::last_os_error()
            );
            Timer(Arc::new(Inner {
                queue: Arc::clone(&self.queue),
                fd,
            }))
        }

        pub fn changed(&self) {
            self.queue.changed();
        }

        pub fn wait(&self, timers: &[Timer]) -> Wake {
            let mut fds = Vec::with_capacity(timers.len() + 1);
            fds.push(libc::pollfd {
                fd: self.queue.changed,
                events: libc::POLLIN,
                revents: 0,
            });
            fds.extend(timers.iter().map(|timer| libc::pollfd {
                fd: timer.0.fd,
                events: libc::POLLIN,
                revents: 0,
            }));
            loop {
                // SAFETY: fds is a live, writable pollfd slice and all descriptors remain open.
                let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
                if ready >= 0 {
                    break;
                }
                assert_eq!(
                    io::Error::last_os_error().kind(),
                    io::ErrorKind::Interrupted,
                    "poll failed: {}",
                    io::Error::last_os_error()
                );
            }

            let mut changed = false;
            let mut fired = None;
            for (index, fd) in fds.iter().enumerate() {
                if fd.revents & libc::POLLIN == 0 {
                    continue;
                }
                let mut expirations = 0_u64;
                // SAFETY: expirations is a valid u64 buffer and every descriptor is poll-readable.
                let read = unsafe {
                    libc::read(
                        fd.fd,
                        (&mut expirations as *mut u64).cast(),
                        size_of::<u64>(),
                    )
                };
                if read < 0 {
                    assert_eq!(
                        io::Error::last_os_error().kind(),
                        io::ErrorKind::WouldBlock,
                        "watchdog timer read failed: {}",
                        io::Error::last_os_error()
                    );
                }
                if index == 0 {
                    changed = true;
                } else if fired.is_none() {
                    fired = Some(index - 1);
                }
            }
            if changed {
                Wake::Changed
            } else {
                Wake::Fired(fired.expect("poll returned a watched timer"))
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod imp {
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    pub enum Wake {
        Changed,
        Fired(usize),
    }

    struct Queue {
        fd: libc::c_int,
        next_timer: AtomicUsize,
    }

    impl Queue {
        fn new() -> io::Result<Queue> {
            // SAFETY: kqueue takes no arguments and the returned descriptor is owned by Queue.
            let fd = unsafe { libc::kqueue() };
            if fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(Queue {
                    fd,
                    next_timer: AtomicUsize::new(1),
                })
            }
        }

        fn submit(
            &self,
            ident: usize,
            filter: i16,
            flags: u16,
            fflags: u32,
            data: isize,
        ) -> io::Result<()> {
            let change = libc::kevent {
                ident,
                filter,
                flags,
                fflags,
                data,
                udata: std::ptr::null_mut(),
            };
            // SAFETY: fd is a live kqueue and change points to one valid kevent.
            let result = unsafe {
                libc::kevent(
                    self.fd,
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
            if result < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        fn changed(&self) {
            self.submit(0, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER, 0)
                .expect("the watchdog's kqueue user event fires");
        }
    }

    impl Drop for Queue {
        fn drop(&mut self) {
            // SAFETY: Queue owns this descriptor and no other owner closes it.
            unsafe { libc::close(self.fd) };
        }
    }

    struct Inner {
        queue: Arc<Queue>,
        ident: usize,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            let _ = self
                .queue
                .submit(self.ident, libc::EVFILT_TIMER, libc::EV_DELETE, 0, 0);
        }
    }

    #[derive(Clone)]
    pub struct Timer(Arc<Inner>);

    impl Timer {
        pub fn arm(&self, duration: Duration) {
            let millis = duration
                .as_nanos()
                .saturating_add(999_999)
                .checked_div(1_000_000)
                .unwrap_or(0)
                .max(1);
            let data = isize::try_from(millis).unwrap_or(isize::MAX);
            self.0
                .queue
                .submit(
                    self.0.ident,
                    libc::EVFILT_TIMER,
                    libc::EV_ADD | libc::EV_ONESHOT,
                    0,
                    data,
                )
                .expect("the watchdog's kqueue timer arms");
        }

        pub fn disarm(&self) {
            match self
                .0
                .queue
                .submit(self.0.ident, libc::EVFILT_TIMER, libc::EV_DELETE, 0, 0)
            {
                Ok(()) => {}
                Err(error) if error.raw_os_error() == Some(libc::ENOENT) => {}
                Err(error) => panic!("the watchdog's kqueue timer disarms: {error}"),
            }
        }

        pub fn remove(&self) {
            self.disarm();
            self.0.queue.changed();
        }
    }

    pub struct Waiter {
        queue: Arc<Queue>,
    }

    fn empty_event() -> libc::kevent {
        // SAFETY: kevent is a C struct whose all-zero representation is valid for output.
        unsafe { std::mem::zeroed() }
    }

    impl Waiter {
        pub fn new() -> Waiter {
            let queue = Arc::new(Queue::new().expect("the watchdog's kqueue opens"));
            queue
                .submit(0, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0, 0)
                .expect("the watchdog's kqueue user event registers");
            Waiter { queue }
        }

        pub fn timer(&self) -> Timer {
            let ident = self.queue.next_timer.fetch_add(1, Ordering::Relaxed);
            assert_ne!(ident, 0, "the watchdog's kqueue timer identifiers wrap");
            Timer(Arc::new(Inner {
                queue: Arc::clone(&self.queue),
                ident,
            }))
        }

        pub fn changed(&self) {
            self.queue.changed();
        }

        pub fn wait(&self, timers: &[Timer]) -> Wake {
            let mut events = vec![empty_event(); timers.len() + 1];
            let ready = loop {
                // SAFETY: fd is a live kqueue and events is a writable kevent slice.
                let ready = unsafe {
                    libc::kevent(
                        self.queue.fd,
                        std::ptr::null(),
                        0,
                        events.as_mut_ptr(),
                        events.len() as i32,
                        std::ptr::null(),
                    )
                };
                if ready >= 0 {
                    break ready as usize;
                }
                assert_eq!(
                    io::Error::last_os_error().kind(),
                    io::ErrorKind::Interrupted,
                    "kevent failed: {}",
                    io::Error::last_os_error()
                );
            };
            let mut fired = None;
            for event in &events[..ready] {
                if event.filter == libc::EVFILT_USER && event.ident == 0 {
                    return Wake::Changed;
                }
                if event.filter == libc::EVFILT_TIMER
                    && let Some(index) =
                        timers.iter().position(|timer| timer.0.ident == event.ident)
                {
                    fired = fired.or(Some(index));
                }
            }
            Wake::Fired(fired.expect("kevent returned a watched timer"))
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::Arc;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CancelWaitableTimer, CreateEventW, CreateWaitableTimerExW, INFINITE, SetEvent,
        SetWaitableTimer, TIMER_ALL_ACCESS, WaitForMultipleObjects,
    };

    pub enum Wake {
        Changed,
        Fired(usize),
    }

    struct Queue {
        changed: usize,
    }

    impl Queue {
        fn new() -> Queue {
            // SAFETY: null security and name create an unnamed auto-reset event owned by Queue.
            let changed = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
            assert_ne!(changed, 0, "the watchdog's Windows event opens");
            Queue {
                changed: changed as usize,
            }
        }

        fn changed(&self) {
            // SAFETY: changed is a live event handle owned by Queue.
            let result = unsafe { SetEvent(self.changed as HANDLE) };
            assert_ne!(result, 0, "the watchdog's Windows event fires");
        }
    }

    impl Drop for Queue {
        fn drop(&mut self) {
            // SAFETY: Queue owns this handle and no other owner closes it.
            unsafe { CloseHandle(self.changed as HANDLE) };
        }
    }

    struct Inner {
        queue: Arc<Queue>,
        handle: usize,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // SAFETY: Inner owns this waitable timer handle and no other owner closes it.
            unsafe { CloseHandle(self.handle as HANDLE) };
        }
    }

    #[derive(Clone)]
    pub struct Timer(Arc<Inner>);

    impl Timer {
        pub fn arm(&self, duration: Duration) {
            let ticks = duration.as_nanos().saturating_add(99) / 100;
            let ticks = i64::try_from(ticks).unwrap_or(i64::MAX).max(1);
            let due = -ticks;
            // SAFETY: handle is a live waitable timer, due is valid, and no completion routine is used.
            let result = unsafe {
                SetWaitableTimer(self.0.handle as HANDLE, &due, 0, None, std::ptr::null(), 0)
            };
            assert_ne!(result, 0, "the watchdog's Windows timer arms");
        }

        pub fn disarm(&self) {
            // SAFETY: handle is a live waitable timer owned by Inner.
            let result = unsafe { CancelWaitableTimer(self.0.handle as HANDLE) };
            assert_ne!(result, 0, "the watchdog's Windows timer disarms");
        }

        pub fn remove(&self) {
            self.disarm();
            self.0.queue.changed();
        }
    }

    pub struct Waiter {
        queue: Arc<Queue>,
    }

    impl Waiter {
        pub fn new() -> Waiter {
            Waiter {
                queue: Arc::new(Queue::new()),
            }
        }

        pub fn timer(&self) -> Timer {
            // SAFETY: null security and name create an unnamed waitable timer owned by Inner.
            let handle = unsafe {
                CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 0, TIMER_ALL_ACCESS)
            };
            assert_ne!(handle, 0, "the watchdog's Windows timer opens");
            Timer(Arc::new(Inner {
                queue: Arc::clone(&self.queue),
                handle: handle as usize,
            }))
        }

        pub fn changed(&self) {
            self.queue.changed();
        }

        pub fn wait(&self, timers: &[Timer]) -> Wake {
            let mut handles = Vec::with_capacity(timers.len() + 1);
            handles.push(self.queue.changed as HANDLE);
            handles.extend(timers.iter().map(|timer| timer.0.handle as HANDLE));
            // SAFETY: handles contains live event and waitable timer handles for the whole wait.
            let result = unsafe {
                WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE)
            };
            assert_ne!(result, WAIT_FAILED, "the watchdog's Windows wait succeeds");
            if result == WAIT_OBJECT_0 {
                return Wake::Changed;
            }
            let index = result
                .checked_sub(WAIT_OBJECT_0 + 1)
                .map(|index| index as usize)
                .filter(|&index| index < timers.len())
                .expect("WaitForMultipleObjects returned a watched timer");
            Wake::Fired(index)
        }
    }
}

pub(super) use imp::{Timer, Waiter, Wake};
