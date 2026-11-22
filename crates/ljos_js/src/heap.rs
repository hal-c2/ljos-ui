//! The runtime's allocator: the system's, as QuickJS's own default, but
//! counting what it holds.
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
pub(crate) struct HeapCount(Rc<Cell<usize>>);

impl HeapCount {
    pub(crate) fn bytes(&self) -> usize {
        self.0.get()
    }

    fn add(&self, bytes: usize) {
        self.0.set(self.bytes().saturating_add(bytes));
    }

    fn sub(&self, bytes: usize) {
        self.0.set(self.bytes().saturating_sub(bytes));
    }
}

unsafe extern "C" {
    // Ljós's patch to the vendored quickjs.c (quickjs.h).
    fn JS_MarkMallocPeak(rt: *mut rquickjs::qjs::JSRuntime) -> usize;
    fn JS_ResumeMallocPeak(rt: *mut rquickjs::qjs::JSRuntime, outer: usize) -> usize;
    fn JS_GCObjectDigest(rt: *mut rquickjs::qjs::JSRuntime) -> u64;
    fn JS_NullErrorCount(rt: *mut rquickjs::qjs::JSRuntime) -> u64;
    fn JS_SetMallocCap(rt: *mut rquickjs::qjs::JSRuntime, cap: usize);
    fn JS_MallocCapRefused(rt: *mut rquickjs::qjs::JSRuntime) -> bool;
    fn JS_SetMallocCapRefused(rt: *mut rquickjs::qjs::JSRuntime, refused: bool);
    fn JS_PauseMallocCap(rt: *mut rquickjs::qjs::JSRuntime) -> usize;
    fn JS_ResumeMallocCap(rt: *mut rquickjs::qjs::JSRuntime, since: usize, keep: bool);
    fn JS_TriggeredGC(rt: *mut rquickjs::qjs::JSRuntime, freed: *mut usize) -> u64;
}

/// The collections QuickJS triggered itself, and what they freed in all.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Triggered {
    pub(crate) runs: u64,
    pub(crate) freed: usize,
}

/// What the runtime went through in a run: the most it held, in the
/// units QuickJS checks its limit in (`malloc_size`, block by block),
/// whether it was refused room past its share, and whether QuickJS threw
/// `null` for want of room to make an error. A
/// script can throw `null` itself, on a heap as full as it likes: only the
/// second tells the two apart.
pub(crate) struct MallocPeak {
    rt: *mut rquickjs::qjs::JSRuntime,
    /// `JS_NullErrorCount` as the run began.
    nulls_from: Cell<u64>,
}

/// The most the runtime held in a run, in the units QuickJS checks its
/// limit in (`malloc_size`, block by block). A function that ran out frees
/// its locals as its exception unwinds it, so an unworded `InternalError`
/// is judged against this, not against what is held after.
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
        unsafe { JS_ResumeMallocPeak(self.rt, 0) }
    }

    /// Holds the run to `cap` (`JsConfig::dispatch_heap_share`), which is
    /// in the limit's units, so it is QuickJS's allocators that refuse:
    /// [`Counted`] sees whole arenas, and nothing of the blocks reused in
    /// them. Not the interrupt handler either: QuickJS asks that only
    /// every 10k opcodes, and a loop that allocates as it goes is past any
    /// share by then.
    pub(crate) fn set_cap(&self, cap: Option<usize>) {
        // SAFETY: a live runtime (`of`); a runtime always holds something,
        // so a cap is never the 0 that means none.
        unsafe { JS_SetMallocCap(self.rt, cap.unwrap_or(0)) };
    }

    /// Sees that the collector runs before the run is refused room at
    /// `cap`; called with [`MallocPeak::set_cap`], as a run begins.
    /// QuickJS sets its next collection from what lived at the last one,
    /// and only then: once the heap shrank, or near the limit, that can be
    /// past the cap, and garbage in cycles would be told as the heap running
    /// out. It is only ever brought down, and to the last eighth of the
    /// share: lower, a run that comes near the same height each time would
    /// collect each time.
    pub(crate) fn collect_before(&self, cap: usize) {
        let last = cap - cap.saturating_sub(self.get()) / 8;
        // SAFETY: a live runtime (`of`); both only read or write a field.
        unsafe {
            if rquickjs::qjs::JS_GetGCThreshold(self.rt) as usize > last {
                rquickjs::qjs::JS_SetGCThreshold(self.rt, last as _);
            }
        }
    }

    /// What QuickJS's own trigger collected since the runtime began.
    pub(crate) fn triggered(&self) -> Triggered {
        let mut freed = 0;
        // SAFETY: a live runtime (`of`), and `freed` is written once.
        let runs = unsafe { JS_TriggeredGC(self.rt, &mut freed) };
        Triggered { runs, freed }
    }

    /// Begins what is not the dispatch's: an engine's registration of a
    /// type of its own (`TypeDef::engine_owned`). Until the guard goes,
    /// only the limit holds it. QuickJS ends that sooner when it throws,
    /// before it makes the error, as making one can run the script's
    /// `Error.prepareStackTrace`; nothing else begun here may run script.
    pub(crate) fn outside_share(&self) -> OutsideShare<'_> {
        // SAFETY: a live runtime (`of`).
        let since = unsafe { JS_PauseMallocCap(self.rt) };
        OutsideShare {
            peak: self,
            since,
            keep: false,
        }
    }

    /// Whether room was refused for being past the cap since it was last
    /// cleared ([`MallocPeak::clear_over_share`]).
    pub(crate) fn over_share(&self) -> bool {
        // SAFETY: a live runtime (`of`).
        unsafe { JS_MallocCapRefused(self.rt) }
    }

    /// Forgets a refusal: done where a dispatch, a job or a nested run
    /// begins, and where a dispatch or a job ends.
    pub(crate) fn clear_over_share(&self) {
        // SAFETY: a live runtime (`of`).
        unsafe { JS_SetMallocCapRefused(self.rt, false) };
    }

    /// Ends a nested run: the run around it was refused room if it was
    /// before the nested run began, or if the nested run was, since that
    /// ran inside it. So a nested run that fails for its own reason leaves
    /// the run around it knowing it was refused, and is judged by its own
    /// refusals only.
    pub(crate) fn resume_over_share(&self, outer: bool) {
        let refused = outer || self.over_share();
        // SAFETY: a live runtime (`of`).
        unsafe { JS_SetMallocCapRefused(self.rt, refused) };
    }

    /// Whether QuickJS threw `null` since the last [`MallocPeak::mark`]
    /// because it had no room to make the error it meant to throw.
    pub(crate) fn ran_out(&self) -> bool {
        // SAFETY: a live runtime (`of`).
        let now = unsafe { JS_NullErrorCount(self.rt) };
        now != self.nulls_from.get()
    }
}

/// While it lives, what is made is not the dispatch's
/// ([`MallocPeak::outside_share`]).
pub(crate) struct OutsideShare<'a> {
    peak: &'a MallocPeak,
    since: usize,
    keep: bool,
}

impl OutsideShare<'_> {
    /// Ends it with what was made kept: the cap moves up by as much. One
    /// dropped without this moves nothing, so what a failure left is the
    /// dispatch's.
    pub(crate) fn keep(mut self) {
        self.keep = true;
    }
}

impl Drop for OutsideShare<'_> {
    fn drop(&mut self) {
        // SAFETY: a live runtime (`of`).
        unsafe { JS_ResumeMallocCap(self.peak.rt, self.since, self.keep) };
    }
}

/// A digest of the runtime's collectable objects and their reference
/// counts. Equal digests mean, to a 64-bit hash, the same objects at the
/// same addresses with the same counts, and nothing more: what references
/// what is not in it. A run that drops the last reference to a cycle
/// frees nothing, so neither the heap nor its peak tells, but a count
/// usually changes. Not always: `a.next.next = a.next; a.next = a;
/// globalThis.a = null` leaves both objects unreachable with the counts
/// they had. So a changed digest says something may be garbage, and an
/// equal one is only a good guess that nothing became garbage: it may put
/// a collection off, never refuse one that is owed. Walks the object list
/// once, not what the objects hold, so it is far cheaper than a
/// collection, but it is not free.
pub(crate) struct ObjectDigest(*mut rquickjs::qjs::JSRuntime);

impl ObjectDigest {
    /// # Safety
    ///
    /// `rt` is a live runtime, and outlives what this is handed to.
    pub(crate) unsafe fn of(rt: *mut rquickjs::qjs::JSRuntime) -> Self {
        Self(rt)
    }

    pub(crate) fn get(&self) -> u64 {
        // SAFETY: a live runtime (`of`); the walk reads the list only.
        unsafe { JS_GCObjectDigest(self.0) }
    }
}

impl std::fmt::Debug for ObjectDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ObjectDigest")
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
        // SAFETY: plain malloc; a null result is passed on.
        let ptr = unsafe { system::malloc(size) }.cast::<u8>();
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { system::usable(ptr) });
        }
        ptr
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
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
        let ptr = rquickjs::allocator::RustAllocator.alloc(size);
        if !ptr.is_null() {
            // SAFETY: just allocated.
            self.count.add(unsafe { Self::usable_size(ptr) });
        }
        ptr
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
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

    /// The allocator's rounding can leave the heap past a cap set just
    /// above what was asked for: giving memory back must still work then.
    #[test]
    fn a_block_shrinks_when_the_heap_is_past_its_cap() {
        use rquickjs::qjs;
        // SAFETY: a runtime of the test's own, used here only and freed
        // last, after the one block taken from it.
        unsafe {
            let rt = qjs::JS_NewRuntime();
            assert!(!rt.is_null());
            let heap = MallocPeak::of(rt);
            let block = qjs::js_malloc_rt(rt, 4096);
            assert!(!block.is_null());
            heap.set_cap(Some(1));
            assert!(qjs::js_malloc_rt(rt, 16).is_null(), "room past the cap");
            assert!(heap.over_share());
            heap.clear_over_share();
            let smaller = qjs::js_realloc_rt(rt, block, 1024);
            assert!(!smaller.is_null(), "a shrink past the cap was refused");
            assert!(!heap.over_share(), "a shrink is not a refusal");
            qjs::js_free_rt(rt, smaller);
            qjs::JS_FreeRuntime(rt);
        }
    }
}
