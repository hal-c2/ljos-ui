//! The process on Linux, from `/proc` as procfs reads it.

use procfs::process::{Process as Proc, Task};

use super::{Process, Resident, Thread};

pub(super) const NAME_KEPT: usize = 15;

pub(super) fn current_tid() -> Option<u64> {
    std::fs::read_link("/proc/thread-self")
        .ok()?
        .file_name()?
        .to_str()?
        .parse()
        .ok()
}

pub(super) fn process() -> Process {
    let Ok(me) = Proc::myself() else {
        return Process::default();
    };
    Process {
        threads: threads(&me),
        resident: resident_of(&me),
        // utime and stime of every thread there was, ended ones too.
        cpu_us: me.stat().ok().map(|s| ticks_us(s.utime + s.stime)),
        wakeups: None,
    }
}

pub(super) fn resident() -> Option<Resident> {
    resident_of(&Proc::myself().ok()?)
}

/// Microseconds of clock ticks, which `stat`'s times are counted in.
fn ticks_us(ticks: u64) -> u64 {
    ticks * 1_000_000 / procfs::ticks_per_second().max(1)
}

fn threads(me: &Proc) -> Option<Vec<Thread>> {
    let mut all: Vec<Thread> = me.tasks().ok()?.filter_map(|t| thread(&t.ok()?)).collect();
    // The kernel walks a task directory by position, so a thread that
    // ends meanwhile can make the walk skip a living one. The calling
    // thread is not ending, so it is read by its number when skipped.
    if let Some(tid) = current_tid()
        && !all.iter().any(|t| t.tid == tid)
        && let Some(me) = i32::try_from(tid)
            .ok()
            .and_then(|tid| me.task_from_tid(tid).ok())
            .and_then(|t| thread(&t))
    {
        all.push(me);
    }
    all.sort_by_key(|t| t.tid);
    Some(all)
}

fn thread(task: &Task) -> Option<Thread> {
    let stat = task.stat().ok()?;
    // The scheduler's own count in nanoseconds is finer than the ticks of
    // utime and stime, when the kernel keeps it.
    let work_us = task.schedstat().map_or_else(
        |_| ticks_us(stat.utime + stat.stime),
        |s| s.sum_exec_runtime / 1000,
    );
    let status = task.status().ok();
    Some(Thread {
        tid: u64::try_from(task.tid).ok()?,
        name: stat.comm,
        state: Some(stat.state),
        work_us,
        wakeups: status.as_ref().and_then(|s| s.voluntary_ctxt_switches),
        preempted: status.as_ref().and_then(|s| s.nonvoluntary_ctxt_switches),
    })
}

/// What is resident, of it what is a file's or shared, and the rest the
/// process's own. The kernel keeps these counts, so reading them walks
/// nothing.
fn resident_of(me: &Proc) -> Option<Resident> {
    let statm = me.statm().ok()?;
    let page = procfs::page_size();
    let rss = statm.resident * page;
    let shared = statm.shared * page;
    Some(Resident {
        virt: statm.size * page,
        rss,
        shared,
        own: rss.saturating_sub(shared),
    })
}
