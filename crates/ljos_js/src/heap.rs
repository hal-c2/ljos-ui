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
pub(crate) struct HeapCount(Rc<Count>);

#[derive(Debug, Default)]
struct Count {
    bytes: Cell<usize>,
}

impl HeapCount {
    pub(crate) fn bytes(&self) -> usize {
        self.0.bytes.get()
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
    fn JS_GCObjectDigest(rt: *mut rquickjs::qjs::JSRuntime) -> u64;
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
