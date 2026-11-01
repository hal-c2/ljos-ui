//! Telling when the process that started this one has gone, from what the
//! system says of it: the thread that asks sleeps until then, and a run at
//! rest stays at no work at all.
//!
//! The parent is taken hold of before anything slow happens, so that one
//! that dies while the application loads is still noticed. Where nothing can
//! be held ([`Parent::of_this_process`] answers `None`) the run goes on as
//! it would have without this.

/// A hold on the process that started this one.
#[derive(Debug)]
pub struct Parent(imp::Hold);

impl Parent {
    /// The parent, or `None` for a process without one to follow: started
    /// by the system's first process, or on a system that cannot say.
    pub fn of_this_process() -> Option<Self> {
        imp::hold().map(Self)
    }

    /// Returns when the parent has gone.
    pub fn wait(self) {
        // A system that fails to answer has not said the parent went, and
        // that is no reason to end a run.
        if !imp::wait(self.0) {
            loop {
                std::thread::park();
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::os::fd::{FromRawFd, OwnedFd};

    /// `None` when the parent went before it could be held.
    pub type Hold = Option<OwnedFd>;

    pub fn hold() -> Option<Hold> {
        // SAFETY: getppid takes nothing and cannot fail.
        let parent = unsafe { libc::getppid() };
        if parent <= 1 {
            return None;
        }
        // Not PR_SET_PDEATHSIG: that fires when the thread that started this
        // process ends, which a parent with many threads does while it lives.
        // SAFETY: the call takes a pid and flags and returns a descriptor,
        // which is close-on-exec, so a process the application starts does
        // not keep it.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, parent, 0) };
        if fd < 0 {
            let gone = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            return gone.then_some(None);
        }
        // SAFETY: the call returned this descriptor, and nothing else has it.
        let held = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        // The number may have been another's by the time it was opened; while
        // it is still this process's parent it cannot have been.
        // SAFETY: as above.
        Some((unsafe { libc::getppid() } == parent).then_some(held))
    }

    pub fn wait(hold: Hold) -> bool {
        use std::os::fd::AsRawFd;
        let Some(fd) = hold else { return true };
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            // SAFETY: one valid pollfd, which outlives the call; a descriptor
            // of a process becomes readable when the process has ended.
            let ready = unsafe { libc::poll(&raw mut poll, 1, -1) };
            if ready > 0 {
                return true;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return false;
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    /// `None` when the parent went before it could be held.
    pub type Hold = Option<OwnedFd>;

    pub fn hold() -> Option<Hold> {
        // SAFETY: getppid takes nothing and cannot fail.
        let parent = unsafe { libc::getppid() };
        if parent <= 1 {
            return None;
        }
        // SAFETY: takes nothing; the queue is not inherited by a fork.
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return None;
        }
        // SAFETY: the call returned this descriptor, and nothing else has it.
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        let change = libc::kevent {
            ident: parent as libc::uintptr_t,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: one valid change and no events asked for.
        let added = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                &raw const change,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if added < 0 {
            let gone = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            return gone.then_some(None);
        }
        // SAFETY: as above. See Linux: the number may have been another's.
        Some((unsafe { libc::getppid() } == parent).then_some(queue))
    }

    pub fn wait(hold: Hold) -> bool {
        let Some(queue) = hold else { return true };
        // SAFETY: an all-zero kevent is a valid value of the plain struct.
        let mut event: libc::kevent = unsafe { std::mem::zeroed() };
        loop {
            // SAFETY: room for one event, which outlives the call, and no
            // timeout: this sleeps until the one thing it waits for.
            let ready = unsafe {
                libc::kevent(
                    queue.as_raw_fd(),
                    std::ptr::null(),
                    0,
                    &raw mut event,
                    1,
                    std::ptr::null(),
                )
            };
            if ready > 0 {
                return true;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return false;
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, GetLastError, HANDLE,
            INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        },
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, INFINITE, OpenProcess,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
            },
        },
    };

    /// `None` when the parent went before it could be held.
    pub type Hold = Option<Handle>;

    #[derive(Debug)]
    pub struct Handle(HANDLE);

    // SAFETY: a process handle may be used from any thread.
    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: this owns the handle, and it is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The system keeps no parent to ask for, only the number it was given
    /// when this process started, among all the processes it lists.
    fn parent_id() -> Option<u32> {
        // SAFETY: takes flags and returns a handle or the invalid one.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let snapshot = Handle(snapshot);
        // SAFETY: an all-zero entry is a valid value of the plain struct.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: takes nothing and cannot fail.
        let own = unsafe { GetCurrentProcessId() };
        // SAFETY: a valid snapshot and an entry whose size is set.
        let mut more = unsafe { Process32FirstW(snapshot.0, &raw mut entry) } != 0;
        while more {
            if entry.th32ProcessID == own {
                return Some(entry.th32ParentProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot.0, &raw mut entry) } != 0;
        }
        None
    }

    fn created(process: HANDLE) -> Option<u64> {
        let mut times = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: a process handle with the right to ask, and four times to
        // fill, which outlive the call.
        let told = unsafe { GetProcessTimes(process, created, exited, kernel, user) };
        (told != 0)
            .then(|| u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
    }

    pub fn hold() -> Option<Hold> {
        let parent = parent_id().filter(|id| *id != 0)?;
        // SAFETY: takes a right and a number; the handle is not inherited.
        let opened = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                parent,
            )
        };
        if opened == 0 {
            // SAFETY: takes nothing. Another error means a parent there is
            // no right to look at, which is not a parent gone.
            let gone = unsafe { GetLastError() } == ERROR_INVALID_PARAMETER;
            return gone.then_some(None);
        }
        let held = Handle(opened);
        // The system gives a number out again while a child of its old holder
        // lives; a process younger than this one cannot be what started it.
        // SAFETY: the pseudo handle of this process is always valid.
        let own = created(unsafe { GetCurrentProcess() })?;
        Some((created(held.0)? <= own).then_some(held))
    }

    pub fn wait(hold: Hold) -> bool {
        let Some(process) = hold else { return true };
        // SAFETY: a handle held open with the right to wait on it; a process
        // handle is signalled when the process has ended.
        unsafe { WaitForSingleObject(process.0, INFINITE) == WAIT_OBJECT_0 }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    pub type Hold = ();

    pub fn hold() -> Option<Hold> {
        None
    }

    pub fn wait((): Hold) -> bool {
        false
    }
}
