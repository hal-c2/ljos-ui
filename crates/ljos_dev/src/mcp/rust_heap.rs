//! What Rust code holds on the heap, so that `memory` can split what the
//! allocator has in use and no owner tells of between Rust code and the C
//! libraries (the GPU driver, LLVM, fontconfig, the script engine), which
//! allocate from the same allocator.
//!
//! A program opts in by making [`CountingAlloc`] its global allocator; the
//! `ljos` command does. Each thread counts into a slot of its own with a
//! plain load and store, no locked instruction and nothing shared, so an
//! allocation costs about a nanosecond more. Threads past the slots share
//! the last one, with an atomic add. An answer sums the slots.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering::Relaxed};

const SLOTS: usize = 128;
static COUNTS: [AtomicIsize; SLOTS] = [const { AtomicIsize::new(0) }; SLOTS];
static NEXT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    // No destructor and a constant start: reading it never allocates.
    static SLOT: Cell<usize> = const { Cell::new(usize::MAX) };
}

fn count(bytes: isize) {
    let shared = SLOTS - 1;
    let slot = SLOT
        .try_with(|s| {
            let mut i = s.get();
            if i == usize::MAX {
                i = NEXT.fetch_add(1, Relaxed).min(shared);
                s.set(i);
            }
            i
        })
        .unwrap_or(shared);
    let c = &COUNTS[slot];
    if slot == shared {
        c.fetch_add(bytes, Relaxed);
    } else {
        // Only this thread writes its slot.
        c.store(c.load(Relaxed) + bytes, Relaxed);
    }
}

/// The system allocator, counting what Rust code holds.
#[derive(Debug)]
pub struct CountingAlloc;

// SAFETY: every call is the system allocator's, with the same arguments;
// the counting touches no memory it hands out.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as the caller promises for `alloc`.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            count(layout.size() as isize);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as the caller promises for `alloc_zeroed`.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            count(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as the caller promises for `dealloc`.
        unsafe { System.dealloc(ptr, layout) };
        count(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: as the caller promises for `realloc`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            count(new_size as isize - layout.size() as isize);
        }
        p
    }
}

/// The bytes Rust code holds; `None` where [`CountingAlloc`] is not the
/// global allocator, as nothing was counted.
pub(crate) fn rust_heap_bytes() -> Option<u64> {
    let sum: isize = COUNTS.iter().map(|c| c.load(Relaxed)).sum();
    (sum > 0).then_some(sum as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_allocated_and_freed_through_it_is_counted() {
        let before = COUNTS.iter().map(|c| c.load(Relaxed)).sum::<isize>();
        let layout = Layout::from_size_align(4096, 8).unwrap();
        // SAFETY: a non-zero layout, freed with the same layout below.
        let p = unsafe { CountingAlloc.alloc(layout) };
        assert!(!p.is_null());
        let held = COUNTS.iter().map(|c| c.load(Relaxed)).sum::<isize>();
        assert_eq!(held - before, 4096);
        // SAFETY: `p` came from this allocator with `layout`.
        let p = unsafe { CountingAlloc.realloc(p, layout, 8192) };
        let grown = COUNTS.iter().map(|c| c.load(Relaxed)).sum::<isize>();
        assert_eq!(grown - before, 8192);
        // SAFETY: `p` holds 8192 bytes at alignment 8.
        unsafe { CountingAlloc.dealloc(p, Layout::from_size_align(8192, 8).unwrap()) };
        let after = COUNTS.iter().map(|c| c.load(Relaxed)).sum::<isize>();
        assert_eq!(after, before);
    }
}
