//! The process on macOS: libproc for what `proc_pidinfo` and
//! `proc_pid_rusage` tell, Mach for what libproc does not wrap (the ids of
//! the task's threads, and its map).
//!
//! Mach counts no wakeups per thread, so a thread's are left out, not
//! given as 0. The process's interrupt wakeups are its total.

use std::mem::{MaybeUninit, size_of};

use libc::{
    KERN_SUCCESS, TH_FLAGS_IDLE, TH_STATE_HALTED, TH_STATE_RUNNING, TH_STATE_STOPPED,
    TH_STATE_UNINTERRUPTIBLE, THREAD_IDENTIFIER_INFO, integer_t, mach_msg_type_number_t, natural_t,
    thread_act_t, thread_identifier_info,
};
use libproc::pid_rusage::{RUsageInfoV4, pidrusage};
use libproc::proc_pid::{pidinfo, regionfilename};
use libproc::task_info::TaskInfo;
use libproc::thread_info::ThreadInfo;

use super::{Process, Resident, Thread};
use crate::mcp::mappings::Region;

/// Mach keeps 63 bytes of a thread's name.
pub(super) const NAME_KEPT: usize = 63;

fn pid() -> i32 {
    i32::try_from(std::process::id()).unwrap_or(i32::MAX)
}

fn task() -> libc::mach_port_t {
    // SAFETY: the task's own port, which the process holds for its life;
    // asking for it takes no reference.
    #[allow(deprecated)]
    unsafe {
        libc::mach_task_self()
    }
}

pub(super) fn current_tid() -> Option<u64> {
    let mut id = 0u64;
    // SAFETY: `pthread_self` is the calling thread, which is alive.
    let r = unsafe { libc::pthread_threadid_np(libc::pthread_self(), &mut id) };
    (r == 0).then_some(id)
}

pub(super) fn process() -> Process {
    let task = pidinfo::<TaskInfo>(pid(), 0).ok();
    let usage = pidrusage::<RUsageInfoV4>(pid()).ok();
    Process {
        threads: Some(threads()),
        resident: task
            .as_ref()
            .zip(usage.as_ref())
            .map(|(t, u)| resident_of(t, u)),
        // Every thread there was, the ended ones too.
        cpu_us: task.map(|t| mach_us(t.pti_total_user + t.pti_total_system)),
        wakeups: usage.map(|u| u.ri_interrupt_wkups),
    }
}

pub(super) fn resident() -> Option<Resident> {
    let task = pidinfo::<TaskInfo>(pid(), 0).ok()?;
    let usage = pidrusage::<RUsageInfoV4>(pid()).ok()?;
    Some(resident_of(&task, &usage))
}

/// The process's own is the footprint the system charges it (what it
/// dirtied, compressed or not), as Activity Monitor shows; the rest of
/// what is resident is files and memory shared with others.
fn resident_of(task: &TaskInfo, usage: &RUsageInfoV4) -> Resident {
    let rss = usage.ri_resident_size;
    let own = usage.ri_phys_footprint;
    Resident {
        virt: task.pti_virtual_size,
        rss,
        shared: rss.saturating_sub(own),
        own,
    }
}

/// Microseconds of the task's times, which are counted in Mach's
/// absolute time units: nanoseconds on Intel, but not on Apple silicon.
fn mach_us(ticks: u64) -> u64 {
    let mut base = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: the out pointer is valid.
    let ok = unsafe { mach2::mach_time::mach_timebase_info(&mut base) } == KERN_SUCCESS;
    let (numer, denom) = if ok && base.denom != 0 {
        (u128::from(base.numer), u128::from(base.denom))
    } else {
        (1, 1)
    };
    u64::try_from(u128::from(ticks) * numer / denom / 1000).unwrap_or(u64::MAX)
}

/// Every thread of the process, by its system-wide id: the one `sample`,
/// Instruments and a debugger name it by.
fn threads() -> Vec<Thread> {
    let mut all: Vec<Thread> = ids()
        .into_iter()
        .filter_map(|(tid, handle)| thread(tid, handle))
        .collect();
    all.sort_by_key(|t| t.tid);
    all
}

/// Each thread's id and handle (the address of its pthread, which
/// `proc_pidinfo` takes), from the task's threads: libproc lists the
/// handles alone.
fn ids() -> Vec<(u64, u64)> {
    let task = task();
    let mut list: *mut thread_act_t = std::ptr::null_mut();
    let mut count: mach_msg_type_number_t = 0;
    // SAFETY: the out pointers are valid; on success the kernel hands over
    // an array of `count` send rights, given back below.
    if unsafe { libc::task_threads(task, &mut list, &mut count) } != KERN_SUCCESS || list.is_null()
    {
        return Vec::new();
    }
    // SAFETY: `task_threads` wrote `count` ports at `list`.
    let ports = unsafe { std::slice::from_raw_parts(list, count as usize) };
    let ids = ports.iter().filter_map(|port| identifier(*port)).collect();
    for port in ports {
        // SAFETY: each port is a send right the list gave this task.
        unsafe { mach2::mach_port::mach_port_deallocate(task, *port) };
    }
    // SAFETY: the array was allocated in this task's map by the kernel,
    // `count` ports long, and nothing reads it after this.
    unsafe {
        libc::vm_deallocate(
            task,
            list as libc::vm_address_t,
            count as usize * size_of::<thread_act_t>(),
        )
    };
    ids
}

/// The id and handle of the thread `port`, or None if it has ended.
fn identifier(port: thread_act_t) -> Option<(u64, u64)> {
    let mut info = MaybeUninit::<thread_identifier_info>::zeroed();
    let mut count =
        (size_of::<thread_identifier_info>() / size_of::<integer_t>()) as mach_msg_type_number_t;
    // SAFETY: `info` has room for `count` integers, the flavor's size.
    let r = unsafe {
        libc::thread_info(
            port,
            THREAD_IDENTIFIER_INFO as libc::thread_flavor_t,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if r != KERN_SUCCESS {
        return None;
    }
    // SAFETY: plain integers, zeroed where the kernel wrote none.
    let info = unsafe { info.assume_init() };
    Some((info.thread_id, info.thread_handle))
}

fn thread(tid: u64, handle: u64) -> Option<Thread> {
    let info = pidinfo::<ThreadInfo>(pid(), handle).ok()?;
    let name: Vec<u8> = info.pth_name.iter().map(|c| *c as u8).collect();
    let name = std::ffi::CStr::from_bytes_until_nul(&name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(Thread {
        tid,
        name,
        state: Some(state(info.pth_run_state, info.pth_flags)),
        // Nanoseconds, from the thread's own times.
        work_us: (info.pth_user_time + info.pth_system_time) / 1000,
        wakeups: None,
        preempted: None,
    })
}

/// Mach's run state as the letters `/proc` uses, which the rest reads.
fn state(run_state: i32, flags: i32) -> char {
    match run_state {
        TH_STATE_RUNNING => 'R',
        TH_STATE_STOPPED => 'T',
        TH_STATE_UNINTERRUPTIBLE => 'D',
        TH_STATE_HALTED => 'X',
        _ if flags & TH_FLAGS_IDLE != 0 => 'I',
        _ => 'S',
    }
}

/// Every region of the task's map, submaps opened (the shared cache of
/// the system's libraries is one), with the file each maps. None if the
/// first region cannot be read.
pub(crate) fn regions() -> Option<Vec<Region>> {
    use mach2::vm_region::{SM_EMPTY, SM_SHARED, SM_TRUESHARED, vm_region_submap_info_64};
    // SAFETY: no arguments; the size is fixed for the life of the process.
    let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    let mut all = Vec::new();
    let mut address: u64 = 0;
    let mut depth: natural_t = 0;
    loop {
        let mut size: u64 = 0;
        let mut info = MaybeUninit::<vm_region_submap_info_64>::zeroed();
        let mut count = vm_region_submap_info_64::count();
        // SAFETY: the out pointers are valid and `info` has room for
        // `count` integers.
        let r = unsafe {
            mach2::vm::mach_vm_region_recurse(
                task(),
                &mut address,
                &mut size,
                &mut depth,
                info.as_mut_ptr().cast(),
                &mut count,
            )
        };
        if r != KERN_SUCCESS {
            // Past the last region.
            break;
        }
        // SAFETY: plain integers, written by the kernel above.
        let info = unsafe { info.assume_init() };
        if info.is_submap != 0 {
            depth += 1;
            continue;
        }
        if info.share_mode != SM_EMPTY && info.pages_resident > 0 {
            all.push(Region {
                path: regionfilename(pid(), address).unwrap_or_default(),
                exec: info.protection & mach2::vm_prot::VM_PROT_EXECUTE != 0,
                tag: info.user_tag,
                shared: matches!(info.share_mode, SM_SHARED | SM_TRUESHARED),
                resident: u64::from(info.pages_resident) * page,
                dirtied: u64::from(info.pages_dirtied) * page,
            });
        }
        address = address.saturating_add(size);
    }
    (!all.is_empty()).then_some(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_has_the_program_and_the_heap() {
        let block = std::hint::black_box(vec![1u8; 1 << 20]);
        let regions = regions().unwrap();
        let exe = std::env::current_exe().unwrap();
        let exe = exe.to_string_lossy();
        assert!(regions.iter().any(|r| r.path == exe), "{regions:?}");
        assert!(
            regions
                .iter()
                .any(|r| r.path.is_empty() && (1..=13).contains(&r.tag))
        );
        drop(block);
    }

    #[test]
    fn the_process_counts_its_interrupt_wakeups() {
        assert!(process().wakeups.is_some());
    }
}
