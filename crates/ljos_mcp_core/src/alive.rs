//! Whether the process a token file names still runs, asked of the system
//! itself rather than of `/proc`, which only Linux has.

/// Whether process `pid` runs: `Some(false)` when the system says no such
/// process is there, `None` where that cannot be told.
///
/// A process that exited and whose pid was given to another one counts as
/// running, as it does for any check by pid.
pub fn process_alive(pid: u32) -> Option<bool> {
    imp::alive(pid)
}

#[cfg(unix)]
mod imp {
    pub(super) fn alive(pid: u32) -> Option<bool> {
        // A pid the kernel's type cannot hold names no process; 0 and
        // negative numbers name groups to `kill`, not a process.
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return Some(false);
        };
        if pid <= 0 {
            return None;
        }
        // Signal 0 is only the checks a signal goes through.
        // SAFETY: `kill` with signal 0 sends nothing and touches no memory.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return Some(true);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::ESRCH) => Some(false),
            // It is there, and belongs to someone else.
            Some(libc::EPERM) => Some(true),
            _ => None,
        }
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub(super) fn alive(pid: u32) -> Option<bool> {
        // SAFETY: plain calls on a handle this function opens and closes.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle == 0 {
                return match GetLastError() {
                    // No process has that id.
                    ERROR_INVALID_PARAMETER => Some(false),
                    ERROR_ACCESS_DENIED => Some(true),
                    _ => None,
                };
            }
            let mut code = 0u32;
            let read = GetExitCodeProcess(handle, &mut code);
            CloseHandle(handle);
            // A process that has exited keeps its object while a handle to
            // it is open; its exit code says it is done.
            (read != 0).then_some(code != STILL_ACTIVE as u32)
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub(super) fn alive(_pid: u32) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::process_alive;

    #[test]
    fn this_process_runs() {
        assert_eq!(process_alive(std::process::id()), Some(true));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn a_process_that_exited_does_not() {
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", "exit"])
                .spawn()
        } else {
            std::process::Command::new("true").spawn()
        }
        .expect("a child");
        let pid = child.id();
        child.wait().expect("waited");
        assert_eq!(process_alive(pid), Some(false));
    }

    #[cfg(unix)]
    #[test]
    fn a_pid_no_process_can_have_names_none() {
        assert_eq!(process_alive(u32::MAX - 1), Some(false));
    }
}
