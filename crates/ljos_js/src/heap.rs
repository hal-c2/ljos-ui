//! The runtime's allocator: the system's, as QuickJS's own default, but
//! counting what it holds, and holding a dispatch to its share of it.
//!
//! QuickJS keeps that count too, but reads it out only with
//! `JS_ComputeMemoryUsage`, which walks every object, shape and atom. Asked
//! whenever a window rests after a script ran, that walk was most of what
//! a click costs (docs/BENCHMARKS.md). Reading this count costs nothing.

use std::{cell::Cell, rc::Rc};

use rquickjs::allocator::Allocator;

/// Bytes the runtime holds from the system, shared with the host. QuickJS
/// carves small blocks out of arenas, so this is not what its limit is
/// checked against: a block freed in an arena still in use is still held
/// here ([`MallocPeak`] is in the limit's units).
#[derive(Debug, Clone, Default)]
pub(crate) struct HeapCount(Rc<Count>);

#[derive(Debug, Default)]
struct Count {
    bytes: Cell<usize>,
    /// The most the heap may grow to while the dispatch running now runs
    /// (`JsConfig::dispatch_heap_share`). Held here and not in the
    /// interrupt handler: QuickJS asks that only every 10k opcodes, and a
    /// loop that allocates as it goes is past any share by then.
    cap: Cell<Option<usize>>,
    /// Room was refused for being past the cap.
    over_share: Cell<bool>,
}

impl HeapCount {
    pub(crate) fn bytes(&self) -> usize {
        self.0.bytes.get()
    }

    pub(crate) fn set_cap(&self, cap: Option<usize>) {
        self.0.cap.set(cap);
    }

    /// Whether room was refused for being past the cap since it was last
    /// cleared ([`HeapCount::clear_over_share`]).
    pub(crate) fn over_share(&self) -> bool {
        self.0.over_share.get()
    }

    /// Forgets a refusal: done where a dispatch, a job or a nested run
    /// begins, and where a dispatch or a job ends.
    pub(crate) fn clear_over_share(&self) {
        self.0.over_share.set(false);
    }

    /// Ends a nested run: the run around it was refused room if it was
    /// before the nested run began, or if the nested run was, since that
    /// ran inside it. So a nested run that fails for its own reason leaves
    /// the run around it knowing it was refused, and is judged by its own
    /// refusals only.
    pub(crate) fn resume_over_share(&self, outer: bool) {
        self.0.over_share.set(outer || self.over_share());
    }

    /// Whether growing by `more` bytes would pass the cap; noted if so.
    /// QuickJS takes a refusal as the heap running out, as it does at its
    /// own limit.
    fn refuses(&self, more: usize) -> bool {
        let over = self
            .0
            .cap
            .get()
            .is_some_and(|cap| self.bytes().saturating_add(more) > cap);
        if over {
            self.0.over_share.set(true);
        }
        over
    }

    fn add(&self, bytes: usize) {
        self.0.bytes.set(self.bytes().saturating_add(bytes));
    }

    fn sub(&self, bytes: usize) {
        self.0.bytes.set(self.bytes().saturating_sub(bytes));
    }
}

unsafe extern "C" {
    // Ljós's patch to the vendored quickjs.c (quickjs.h).
    fn JS_MarkMallocPeak(rt: *mut rquickjs::qjs::JSRuntime) -> usize;
    fn JS_ResumeMallocPeak(rt: *mut rquickjs::qjs::JSRuntime, outer: usize) -> usize;
}

/// The most the runtime held in a run, in the units QuickJS checks its
/// limit in (`malloc_size`, block by block). A function that ran out frees
/// its locals as its exception unwinds it, so a thrown `null` is judged
/// against this, not against what is held after.
pub(crate) struct MallocPeak(*mut rquickjs::qjs::JSRuntime);

impl MallocPeak {
    /// # Safety
    ///
    /// `rt` is a live runtime, and outlives what this is handed to.
    pub(crate) unsafe fn of(rt: *mut rquickjs::qjs::JSRuntime) -> Self {
        Self(rt)
    }

    /// Starts the high-water mark again from what is held now; a host
    /// does this as a run begins. Returns the mark it replaces, for a
    /// nested run to hand back with [`MallocPeak::resume`].
    pub(crate) fn mark(&self) -> usize {
        // SAFETY: a live runtime (`of`).
        unsafe { JS_MarkMallocPeak(self.0) }
    }

    /// Goes back to the run a nested one interrupted: its mark `outer`,
    /// or the nested run's if that went higher, since the outer run held
    /// the heap then too.
    pub(crate) fn resume(&self, outer: usize) {
        // SAFETY: a live runtime (`of`).
        unsafe { JS_ResumeMallocPeak(self.0, outer) };
    }

    /// The most held since the last [`MallocPeak::mark`].
    pub(crate) fn get(&self) -> usize {
        // SAFETY: a live runtime (`of`); 0 is never above a mark.
        unsafe { JS_ResumeMallocPeak(self.0, 0) }
    }
}

impl std::fmt::Debug for MallocPeak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MallocPeak")
    }
}

pub(crate) struct Counted {
    count: HeapCount,
}

impl Counted {
    pub(crate) fn new(count: HeapCount) -> Self {
        Self { count }
    }
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod system {
    pub(super) use libc::{calloc, free, malloc, realloc};

    pub(super) unsafe fn usable(ptr: *mut u8) -> usize {
        #[cfg(target_vendor = "apple")]
        // SAFETY: `ptr` came from this allocator (caller's contract).
        return unsafe { libc::malloc_size(ptr.cast()) };
        #[cfg(not(target_vendor = "apple"))]
        // SAFETY: as above.
        return unsafe { libc::malloc_usable_size(ptr.cast()) };
    }
}

// SAFETY: every pointer comes from the system allocator, which aligns to
// at least `usize`, and `usable_size` asks that allocator.
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
unsafe impl Allocator for Counted {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if self.count.refuses(size) {
            return std::ptr::null_mut();
        }
        // SAFETY: plain malloc; a null result is passed on.
        let ptr = unsafe { system::malloc(size) }.cast::<u8>();
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { system::usable(ptr) });
        }
        ptr
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        if self.count.refuses(count.saturating_mul(size)) {
            return std::ptr::null_mut();
        }
        // SAFETY: plain calloc, which checks the product itself.
        let ptr = unsafe { system::calloc(count, size) }.cast::<u8>();
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { system::usable(ptr) });
        }
        ptr
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        // SAFETY: the caller hands back a live allocation of ours.
        self.count.sub(unsafe { system::usable(ptr) });
        // SAFETY: as above.
        unsafe { system::free(ptr.cast()) };
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        let old = if ptr.is_null() {
            0
        } else {
            // SAFETY: a live allocation of ours (caller's contract).
            unsafe { system::usable(ptr) }
        };
        // A shrink gives memory back, so it is never refused, even with the
        // count past the cap (the allocator rounds up what it records).
        if new_size > old && self.count.refuses(new_size - old) {
            return std::ptr::null_mut();
        }
        // SAFETY: as above.
        let moved = unsafe { system::realloc(ptr.cast(), new_size) }.cast::<u8>();
        if !moved.is_null() {
            self.count.sub(old);
            // SAFETY: just allocated.
            self.count.add(unsafe { system::usable(moved) });
        } else if new_size == 0 {
            // The C library freed it (QuickJS frees size 0 itself, but the
            // allocator's contract is the C one).
            self.count.sub(old);
        }
        moved
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        // SAFETY: the caller's contract.
        unsafe { system::usable(ptr) }
    }
}

// Elsewhere the C library cannot say how large a block is: Rust's
// allocator, which keeps the size in a header, does.
#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
unsafe impl Allocator for Counted {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if self.count.refuses(size) {
            return std::ptr::null_mut();
        }
        let ptr = rquickjs::allocator::RustAllocator.alloc(size);
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { Self::usable_size(ptr) });
        }
        ptr
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        if self.count.refuses(count.saturating_mul(size)) {
            return std::ptr::null_mut();
        }
        let ptr = rquickjs::allocator::RustAllocator.calloc(count, size);
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { Self::usable_size(ptr) });
        }
        ptr
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        // SAFETY: a live allocation of ours (caller's contract).
        self.count.sub(unsafe { Self::usable_size(ptr) });
        // SAFETY: as above.
        unsafe { rquickjs::allocator::RustAllocator.dealloc(ptr) };
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(new_size);
        }
        // SAFETY: a live allocation of ours (caller's contract).
        let old = unsafe { Self::usable_size(ptr) };
        // A shrink is never refused, as above.
        if new_size > old && self.count.refuses(new_size - old) {
            return std::ptr::null_mut();
        }
        // SAFETY: as above.
        let moved = unsafe { rquickjs::allocator::RustAllocator.realloc(ptr, new_size) };
        if !moved.is_null() {
            self.count.sub(old);
            // SAFETY: just allocated.
            self.count.add(unsafe { Self::usable_size(moved) });
        }
        moved
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        // SAFETY: the caller's contract.
        unsafe { rquickjs::allocator::RustAllocator::usable_size(ptr) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The allocator's rounding can leave the count past a cap set just
    /// above what was asked for: giving memory back must still work then.
    #[test]
    fn a_block_shrinks_when_the_heap_is_past_its_cap() {
        let count = HeapCount::default();
        let mut heap = Counted::new(count.clone());
        let block = heap.alloc(4096);
        assert!(!block.is_null());
        count.set_cap(Some(count.bytes() - 1));
        // SAFETY: `block` is a live allocation of `heap`'s.
        let smaller = unsafe { heap.realloc(block, 1024) };
        assert!(!smaller.is_null(), "a shrink past the cap was refused");
        assert!(!count.over_share(), "a shrink is not a refusal");
        // SAFETY: as above.
        unsafe { heap.dealloc(smaller) };
    }
}
