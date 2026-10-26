//! The process on Windows, from Win32: the Toolhelp snapshot for its
//! threads, each thread's times and description, and the working set.
//!
//! Win32 tells no thread's state and counts no wakeups without
//! `NtQuerySystemInformation`, so those are left out, not given.

use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, K32QueryWorkingSet, PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetProcessTimes,
    GetThreadDescription, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
};

use super::{Process, Resident, Thread};

/// A thread's description is kept whole.
pub(super) const NAME_KEPT: usize = usize::MAX;

pub(super) fn current_tid() -> Option<u64> {
    // SAFETY: no arguments.
    Some(u64::from(unsafe { GetCurrentThreadId() }))
}

pub(super) fn process() -> Process {
    Process {
        threads: threads(),
        resident: resident(),
        cpu_us: cpu_us(),
        wakeups: None,
    }
}

/// Microseconds of a FILETIME span, which counts 100 ns.
fn micros(t: FILETIME) -> u64 {
    ((u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)) / 10
}

/// Kernel and user time of a thread or process handle.
fn times(
    handle: HANDLE,
    get: unsafe extern "system" fn(
        HANDLE,
        *mut FILETIME,
        *mut FILETIME,
        *mut FILETIME,
        *mut FILETIME,
    ) -> i32,
) -> Option<u64> {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: a handle with query rights and four valid out pointers.
    let ok = unsafe { get(handle, &mut created, &mut exited, &mut kernel, &mut user) } != 0;
    ok.then(|| micros(kernel) + micros(user))
}

/// Every thread there was, the ended ones too.
fn cpu_us() -> Option<u64> {
    // SAFETY: the pseudo-handle of this process, which needs no closing.
    times(unsafe { GetCurrentProcess() }, GetProcessTimes)
}

fn threads() -> Option<Vec<Thread>> {
    // SAFETY: a snapshot of every thread of the system, closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: no arguments.
    let me = unsafe { GetCurrentProcessId() };
    // SAFETY: plain integers; the size says which version it is.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut all = Vec::new();
    // SAFETY: a valid snapshot and entry.
    let mut more = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == me {
            all.extend(thread(entry.th32ThreadID));
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(snapshot) };
    all.sort_by_key(|t: &Thread| t.tid);
    Some(all)
}

/// The thread `tid` of this process, or None if it has ended.
fn thread(tid: u32) -> Option<Thread> {
    // SAFETY: opens a handle closed below; 0 if the thread has ended.
    let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, tid) };
    if handle == 0 {
        return None;
    }
    let work_us = times(handle, GetThreadTimes);
    let name = description(handle);
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(handle) };
    Some(Thread {
        tid: u64::from(tid),
        name,
        state: None,
        work_us: work_us?,
        wakeups: None,
        preempted: None,
    })
}

/// The description a thread was given, empty if none.
fn description(handle: HANDLE) -> String {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: on success the system allocates the text, freed below.
    if unsafe { GetThreadDescription(handle, &mut text) } < 0 || text.is_null() {
        return String::new();
    }
    // SAFETY: a NUL-terminated wide string the system wrote.
    let wide = unsafe {
        let len = (0..).take_while(|i| *text.add(*i) != 0).count();
        std::slice::from_raw_parts(text, len)
    };
    let name = String::from_utf16_lossy(wide);
    // SAFETY: allocated by GetThreadDescription with LocalAlloc.
    unsafe { LocalFree(text.cast()) };
    name
}

/// The working set, of it the pages no other process shares, and the
/// address space in use. Telling the shared pages apart walks the working
/// set, a word a page, only when asked.
pub(super) fn resident() -> Option<Resident> {
    // SAFETY: the pseudo-handle of this process, which needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: plain integers; `cb` says which version it is.
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    counters.cb = size;
    // SAFETY: room for `size` bytes.
    if unsafe { K32GetProcessMemoryInfo(process, &mut counters, size) } == 0 {
        return None;
    }
    let rss = counters.WorkingSetSize as u64;
    let own = private_pages(process)? * page_size();
    // SAFETY: plain integers; the length says which version it is.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: a valid out pointer.
    let virt = if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
        status.ullTotalVirtual - status.ullAvailVirtual
    } else {
        rss
    };
    Some(Resident {
        virt,
        rss,
        shared: rss.saturating_sub(own),
        own,
    })
}

fn page_size() -> u64 {
    // SAFETY: plain integers, written whole.
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: a valid out pointer.
    unsafe { GetSystemInfo(&mut info) };
    u64::from(info.dwPageSize)
}

/// Pages of the working set that are not shared.
fn private_pages(process: HANDLE) -> Option<u64> {
    // The first word is the count of entries, each a word after it; the
    // set may grow between asking its size and reading it.
    let mut words = vec![0usize; 2];
    for _ in 0..4 {
        let bytes = (words.len() * std::mem::size_of::<usize>()) as u32;
        // SAFETY: room for `bytes` bytes.
        if unsafe { K32QueryWorkingSet(process, words.as_mut_ptr().cast(), bytes) } != 0 {
            let n = words[0].min(words.len() - 1);
            // Bit 8 of an entry says the page is shared.
            let shared = words[1..=n].iter().filter(|w| *w & (1 << 8) != 0).count();
            return Some((n - shared) as u64);
        }
        words = vec![0usize; words[0] + words[0] / 8 + 64];
    }
    None
}
