//! The measured program as a process: its cgroup, its signals and how it
//! ended.

use std::{
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::Setup;
use crate::{
    cgroup::Cgroup,
    profile::{Profile, cpu_set},
};

/// Checks that `cg` is a cgroup of its own with the profile's limits. A
/// launcher that only starts the command leaves it in the bench's cgroup,
/// which holds the bench too.
pub(super) fn check_isolated(cg: &Path, pid: u32, profile: &Profile) -> Result<(), String> {
    if !cg.is_dir() {
        return Err(format!("its cgroup {} does not exist", cg.display()));
    }
    let c = Cgroup { dir: cg.to_owned() };
    let unreadable = |file: &str| format!("{}/{file} cannot be read", cg.display());
    let procs = c.procs().ok_or_else(|| unreadable("cgroup.procs"))?;
    if procs != [pid] {
        return Err(format!("its cgroup holds {procs:?}, not only {pid}"));
    }
    let l = &profile.limits;
    if let Some(bytes) = l.memory_max {
        let have = c.memory_max().ok_or_else(|| unreadable("memory.max"))?;
        if have != bytes.to_string() {
            return Err(format!("memory.max is {have}, not {bytes}"));
        }
    }
    if l.no_swap {
        let have = c.swap_max().ok_or_else(|| unreadable("memory.swap.max"))?;
        if have != "0" {
            return Err(format!("memory.swap.max is {have}, not 0"));
        }
    }
    if let Some((quota, period)) = profile.cpu_max() {
        let have = c.cpu_max().ok_or_else(|| unreadable("cpu.max"))?;
        if have != format!("{quota} {period}") {
            return Err(format!("cpu.max is {have}, not {quota} {period}"));
        }
    }
    if let Some(want) = &l.cpus {
        let have = c
            .cpus()
            .ok_or_else(|| unreadable("cpuset.cpus.effective"))?;
        if cpu_set(&have) != cpu_set(want) {
            return Err(format!("cpuset.cpus.effective is {have}, not {want}"));
        }
    }
    Ok(())
}

fn signal(pid: u32, sig: i32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: kill(2) on our own child's pid; it cannot be reused while
        // the child is not waited for.
        unsafe {
            libc::kill(pid, sig);
        }
    }
}

/// Ends the child: asked first, then made.
pub(super) fn stop(child: &mut Child) -> Option<ExitStatus> {
    if let Ok(Some(status)) = child.try_wait() {
        return Some(status);
    }
    signal(child.id(), libc::SIGTERM);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

pub(super) fn describe(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("status {code}"),
        (None, Some(sig)) => format!("signal {sig}"),
        _ => "unknown".to_owned(),
    }
}

/// The systemd unit a scope's cgroup is, when the launcher is systemd's.
pub(super) fn unit(setup: &Setup, cg: &Path) -> Option<String> {
    let name = setup.launcher.file_name()?.to_str()?;
    (name == "systemd-run")
        .then(|| cg.file_name()?.to_str().map(str::to_owned))
        .flatten()
}

/// Whether systemd says the unit's process was killed for want of memory.
/// The kernel's count goes with the cgroup once its last process is gone.
pub(super) fn systemd_oom(unit: &str) -> bool {
    let out = Command::new("systemctl")
        .args(["--user", "show", unit, "-p", "Result", "--value"])
        .stderr(Stdio::null())
        .output();
    let oom = out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "oom-kill");
    let _ = Command::new("systemctl")
        .args(["--user", "reset-failed", unit])
        .stderr(Stdio::null())
        .status();
    oom
}
