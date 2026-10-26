//! The process and its threads as Mach tells them, on macOS: what Linux
//! reads from `/proc`. Each answer asks the kernel for counts it keeps
//! anyway; nothing is sampled in between.
//!
//! Mach counts no wakeups per thread, so a thread's are left out, not
//! given as 0. The process's interrupt wakeups are its total.

use std::ffi::CStr;
use std::mem::{MaybeUninit, size_of};

use libc::{
    KERN_SUCCESS, MACH_TASK_BASIC_INFO, PROC_PIDTHREADINFO, TASK_THREAD_TIMES_INFO, TH_FLAGS_IDLE,
    TH_STATE_HALTED, TH_STATE_RUNNING, TH_STATE_STOPPED, TH_STATE_UNINTERRUPTIBLE,
    THREAD_BASIC_INFO, THREAD_IDENTIFIER_INFO, integer_t, mach_msg_type_number_t,
    mach_task_basic_info, natural_t, proc_threadinfo, task_thread_times_info, thread_act_t,
    thread_basic_info, thread_identifier_info, time_value_t,
};
use mach2::task_info::{TASK_POWER_INFO, TASK_VM_INFO, task_power_info, task_vm_info};

use super::{Resident, Thread, Usage};
use crate::mcp::mappings::Region;

fn task() -> libc::mach_port_t {
    // SAFETY: the task's own port, which the process holds for its life;
    // asking for it takes no reference.
    #[allow(deprecated)]
    unsafe {
        libc::mach_task_self()
    }
}

/// Microseconds of a Mach time value.
fn micros(t: time_value_t) -> u64 {
    u64::try_from(t.seconds).unwrap_or(0) * 1_000_000 + u64::try_from(t.microseconds).unwrap_or(0)
}

/// The thread of the caller, as [`threads`] numbers them.
pub(super) fn current_tid() -> Option<u64> {
    let mut id = 0u64;
    // SAFETY: `pthread_self` is the calling thread, which is alive.
    let r = unsafe { libc::pthread_threadid_np(libc::pthread_self(), &mut id) };
    (r == 0).then_some(id)
}

/// Every thread of the process, by its system-wide id.
pub(super) fn threads() -> Vec<Thread> {
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
    let mut all: Vec<Thread> = ports.iter().filter_map(|port| thread(*port)).collect();
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
    all.sort_by_key(|t| t.tid);
    all
}

/// A `thread_info` flavor of `port`, or None if the thread has ended.
fn thread_info<T>(port: thread_act_t, flavor: i32) -> Option<T> {
    let mut info = MaybeUninit::<T>::zeroed();
    let mut count = (size_of::<T>() / size_of::<integer_t>()) as mach_msg_type_number_t;
    // SAFETY: `info` has room for `count` integers, the flavor's size.
    let r = unsafe {
        libc::thread_info(
            port,
            flavor as libc::thread_flavor_t,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    // SAFETY: plain integers, zeroed where the kernel wrote none.
    (r == KERN_SUCCESS).then(|| unsafe { info.assume_init() })
}

fn thread(port: thread_act_t) -> Option<Thread> {
    let basic: thread_basic_info = thread_info(port, THREAD_BASIC_INFO)?;
    let ident: thread_identifier_info = thread_info(port, THREAD_IDENTIFIER_INFO)?;
    Some(Thread {
        tid: ident.thread_id,
        name: name(ident.thread_handle).unwrap_or_default(),
        state: state(basic.run_state, basic.flags),
        work_us: micros(basic.user_time) + micros(basic.system_time),
        wakeups: None,
        preempted: None,
    })
}

/// The name the thread gave itself, from its handle (the address of its
/// pthread). Mach keeps 63 bytes of it.
fn name(handle: u64) -> Option<String> {
    let mut info = MaybeUninit::<proc_threadinfo>::zeroed();
    let size = size_of::<proc_threadinfo>() as libc::c_int;
    // SAFETY: `info` has room for `size` bytes.
    let wrote = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            PROC_PIDTHREADINFO,
            handle,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if wrote != size {
        return None;
    }
    // SAFETY: written whole above.
    let info = unsafe { info.assume_init() };
    let bytes: Vec<u8> = info.pth_name.iter().map(|c| *c as u8).collect();
    let name = CStr::from_bytes_until_nul(&bytes).ok()?;
    Some(name.to_string_lossy().into_owned())
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

/// A `task_info` flavor of this task, and how many integers of it the
/// kernel wrote: an older kernel writes fewer of a struct that grew.
fn task_info<T>(flavor: u32) -> Option<(T, usize)> {
    let mut info = MaybeUninit::<T>::zeroed();
    let mut count = (size_of::<T>() / size_of::<natural_t>()) as mach_msg_type_number_t;
    // SAFETY: `info` has room for `count` integers, the flavor's size.
    let r = unsafe { libc::task_info(task(), flavor, info.as_mut_ptr().cast(), &mut count) };
    // SAFETY: plain integers, zeroed where the kernel wrote none.
    (r == KERN_SUCCESS).then(|| (unsafe { info.assume_init() }, count as usize))
}

/// The process's memory. Its own is the footprint the system charges it
/// (what it dirtied, compressed or not), as Activity Monitor shows; the
/// rest of what is resident is files and memory shared with others.
pub(super) fn resident() -> Option<Resident> {
    let (basic, _) = task_info::<mach_task_basic_info>(MACH_TASK_BASIC_INFO)?;
    let footprint = task_info::<task_vm_info>(TASK_VM_INFO).and_then(|(vm, written)| {
        let end = std::mem::offset_of!(task_vm_info, phys_footprint) + size_of::<u64>();
        (written * size_of::<natural_t>() >= end).then_some(vm.phys_footprint)
    });
    let rss = basic.resident_size;
    // Without the footprint (a kernel too old to give it), all of what
    // is resident is counted as the process's own.
    let own = footprint.unwrap_or(rss);
    Some(Resident {
        virt: basic.virtual_size,
        rss,
        shared: rss.saturating_sub(own),
        own,
    })
}

/// What the process used: memory, the CPU time of every thread there was
/// (the ended ones are in the task's basic info, the living in its thread
/// times), and its interrupt wakeups.
pub(super) fn usage() -> Usage {
    let ended = task_info::<mach_task_basic_info>(MACH_TASK_BASIC_INFO)
        .map(|(b, _)| micros(b.user_time) + micros(b.system_time));
    let living = task_info::<task_thread_times_info>(TASK_THREAD_TIMES_INFO)
        .map(|(t, _)| micros(t.user_time) + micros(t.system_time));
    let wakeups =
        task_info::<task_power_info>(TASK_POWER_INFO).map(|(p, _)| p.task_interrupt_wakeups);
    Usage {
        resident: resident(),
        cpu_us: ended.zip(living).map(|(a, b)| a + b),
        wakeups,
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
                path: region_file(address).unwrap_or_default(),
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

/// The file mapped at `address`, if a file is.
fn region_file(address: u64) -> Option<String> {
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `path` has room for the size given.
    let n = unsafe {
        libc::proc_regionfilename(
            libc::getpid(),
            address,
            path.as_mut_ptr().cast(),
            path.len() as u32,
        )
    };
    let n = usize::try_from(n).ok().filter(|n| *n > 0)?;
    Some(String::from_utf8_lossy(&path[..n.min(path.len())]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_thread_is_listed_by_its_name() {
        let me = current_tid().unwrap();
        let named = std::thread::Builder::new()
            .name("ljos-mach-test".into())
            .spawn(move || {
                let tid = current_tid().unwrap();
                let all = threads();
                (tid, all)
            })
            .unwrap()
            .join()
            .unwrap();
        let (tid, all) = named;
        assert!(all.iter().any(|t| t.tid == me));
        let it = all.iter().find(|t| t.tid == tid).unwrap();
        assert_eq!(it.name, "ljos-mach-test");
        assert_eq!(it.state, 'R');
        assert_eq!(it.wakeups, None);
    }

    #[test]
    fn a_busy_thread_counts_cpu_time() {
        let before = usage().cpu_us.unwrap();
        let start = std::time::Instant::now();
        let mut x = 0u64;
        while start.elapsed().as_millis() < 30 {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        assert!(usage().cpu_us.unwrap() >= before + 10_000);
    }

    #[test]
    fn the_process_has_resident_memory_of_its_own() {
        let r = resident().unwrap();
        assert!(r.rss > 0);
        assert!(r.virt >= r.rss);
        assert!(r.own > 0);
        // Touched pages are resident and the process's own.
        let block = std::hint::black_box(vec![1u8; 8 << 20]);
        let after = resident().unwrap();
        assert!(after.own >= r.own + (4 << 20));
        drop(block);
    }

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
}
