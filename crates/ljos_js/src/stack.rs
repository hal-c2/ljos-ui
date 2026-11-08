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

/// The size to give QuickJS's stack limit on this thread, for
/// [`crate::JsConfig::max_stack`] `given`.
pub(crate) fn for_js(given: Option<usize>) -> usize {
    for_js_at(here(), lowest(), given)
}

/// [`for_js`] with the stack pointer at `sp` and the stack ending at `end`.
fn for_js_at(sp: usize, end: Option<usize>, given: Option<usize>) -> usize {
    let size = match (given, room_at(sp, end)) {
        (Some(most), Some(room)) => most.min(room),
        // QuickJS takes the size from its stack top unsigned, and one past
        // it wraps the limit round so that every call throws. Nothing here
        // says how much stack there is, so no more than the fallback.
        (Some(most), None) => most.min(crate::config::FALLBACK_MAX_STACK),
        (None, Some(room)) => room,
        (None, None) => crate::config::FALLBACK_MAX_STACK,
    };
    // QuickJS reads 0 as no limit at all.
    size.max(1)
}

/// The stack QuickJS may use below `sp`: down to [`RESERVE`] above `end`,
/// the end of the thread's stack. `None` where the end is unknown.
fn room_at(sp: usize, end: Option<usize>) -> Option<usize> {
    sp.checked_sub(end?).map(room_of)
}

/// The address of the caller's frame.
#[inline(always)]
fn here() -> usize {
    let here = std::hint::black_box(0u8);
    (&raw const here) as usize
}

/// The room in `left` bytes of stack. A thread with less than twice the
/// reserve left keeps half of it for native code instead, so that JS still
/// throws before the stack runs out: no room at all would be read as no
/// limit.
fn room_of(left: usize) -> usize {
    if left > 2 * RESERVE {
        left - RESERVE
    } else {
        left / 2
    }
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

    fn room_below_here() -> Option<usize> {
        room_at(here(), lowest())
    }

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
    fn a_small_stack_keeps_half_for_native_code() {
        assert_eq!(room_of(4 << 20), (4 << 20) - RESERVE);
        assert_eq!(room_of(2 * RESERVE + 2), RESERVE + 2);
        assert_eq!(room_of(2 * RESERVE), RESERVE);
        assert_eq!(room_of(RESERVE), RESERVE / 2);
        assert_eq!(room_of(64 * 1024), 32 * 1024);
    }

    /// QuickJS sets its limit at its stack top less the size, unsigned: a
    /// size past the stack pointer wraps the limit round to the top of the
    /// address space, and every call throws. Where the end of the stack is
    /// unknown, nothing says a given size fits, so it is kept to
    /// [`FALLBACK_MAX_STACK`](crate::config::FALLBACK_MAX_STACK).
    #[test]
    fn a_huge_size_given_where_the_end_is_unknown_is_kept_to_the_fallback() {
        let fallback = crate::config::FALLBACK_MAX_STACK;
        let sp = 0x7fff_f000_0000;
        assert_eq!(for_js_at(sp, None, Some(usize::MAX)), fallback);
        assert_eq!(for_js_at(sp, None, Some(64 << 20)), fallback);
        assert_eq!(for_js_at(sp, None, Some(64 * 1024)), 64 * 1024);
        assert_eq!(for_js_at(sp, None, None), fallback);
        assert_eq!(
            for_js_at(sp, Some(sp - (4 << 20)), Some(usize::MAX)),
            (4 << 20) - RESERVE
        );
        assert_eq!(for_js_at(sp, Some(sp - (4 << 20)), Some(0)), 1);
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
        // A stack of its own, well above twice the reserve however deep the
        // test goes: below that, room_of halves what is left and the room
        // shrinks by only half of what the frames take.
        let (shallow, deep) = std::thread::Builder::new()
            .stack_size(4 << 20)
            .spawn(|| (room_below_here(), deeper(16)))
            .unwrap()
            .join()
            .unwrap();
        let (Some(shallow), Some(deep)) = (shallow, deep) else {
            return;
        };
        assert!(
            deep + 16 * 4096 <= shallow,
            "shallow {shallow}, deep {deep}"
        );
    }
}
