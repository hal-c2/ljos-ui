//! Where the calling thread's stack ends, so that QuickJS may use all of it.
//!
//! QuickJS throws a RangeError once the native stack pointer is a set
//! distance below where the runtime was made: rquickjs only moves that mark
//! in its `parallel` build, which Ljós does not use. Everything on the
//! stack in between counts, Rust frames included. Creating an object from
//! JS that creates another from JS adds a whole incubation's frames each
//! time, so a fixed distance (512 KiB) ran out after some 16 levels in a
//! debug build. Then a binding's JS threw while tier 1 gave the right
//! value (hal-c2/ljos-ui#238). The thread's real stack is what may run out,
//! so the limit sits a reserve above its end instead.

/// Stack left below QuickJS's limit for the native code JS calls: a host
/// function that runs when JS is at the limit still has to finish (and may
/// create objects, which costs tens of KiB a level in a debug build) before
/// the next JS call notices the limit and throws.
pub(crate) const RESERVE: usize = 512 * 1024;

/// The stack QuickJS may use below the caller's frame: down to [`RESERVE`]
/// above the end of this thread's stack. `None` where the end is unknown.
pub(crate) fn room_below_here() -> Option<usize> {
    let here = std::hint::black_box(0u8);
    let sp = (&raw const here) as usize;
    let end = lowest()?;
    Some(sp.checked_sub(end)?.saturating_sub(RESERVE))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn lowest() -> Option<usize> {
    // SAFETY: `attr` is filled by `pthread_getattr_np` before it is read and
    // destroyed once, and `pthread_self` is always valid.
    unsafe {
        let mut attr = std::mem::MaybeUninit::<libc::pthread_attr_t>::uninit();
        if libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr()) != 0 {
            return None;
        }
        let mut addr = std::ptr::null_mut();
        let mut size = 0;
        let r = libc::pthread_attr_getstack(attr.as_ptr(), &raw mut addr, &raw mut size);
        libc::pthread_attr_destroy(attr.as_mut_ptr());
        (r == 0 && !addr.is_null()).then_some(addr as usize)
    }
}

#[cfg(target_vendor = "apple")]
fn lowest() -> Option<usize> {
    // SAFETY: both only read the calling thread's own record.
    let (top, size) = unsafe {
        let me = libc::pthread_self();
        (
            libc::pthread_get_stackaddr_np(me) as usize,
            libc::pthread_get_stacksize_np(me),
        )
    };
    top.checked_sub(size).filter(|_| size > 0)
}

#[cfg(windows)]
fn lowest() -> Option<usize> {
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: both are valid for the write.
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(
            &raw mut low,
            &raw mut high,
        );
    }
    (low != 0 && high > low).then_some(low)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
fn lowest() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_has_room_for_most_of_its_stack() {
        let room = std::thread::Builder::new()
            .stack_size(4 << 20)
            .spawn(room_below_here)
            .unwrap()
            .join()
            .unwrap();
        if cfg!(any(
            target_os = "linux",
            target_os = "android",
            target_vendor = "apple",
            windows
        )) {
            let room = room.expect("the thread's stack end is known");
            assert!(room > 3 << 20, "room {room}");
            assert!(room < 4 << 20, "room {room}");
        }
    }

    #[test]
    fn room_shrinks_as_the_stack_deepens() {
        fn deeper(n: usize) -> Option<usize> {
            let pad = std::hint::black_box([0u8; 4096]);
            if n == 0 {
                room_below_here()
            } else {
                deeper(n - 1).map(|r| r + usize::from(pad[0]))
            }
        }
        let (Some(shallow), Some(deep)) = (room_below_here(), deeper(16)) else {
            return;
        };
        assert!(
            deep + 16 * 4096 <= shallow,
            "shallow {shallow}, deep {deep}"
        );
    }
}
