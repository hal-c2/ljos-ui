//! The runtime's allocator: the system's, as QuickJS's own default, but
//! counting what it holds.
//!
//! QuickJS keeps that count too, but reads it out only with
//! `JS_ComputeMemoryUsage`, which walks every object, shape and atom. Asked
//! whenever a window rests after a script ran, that walk was most of what
//! a click costs (docs/BENCHMARKS.md). Reading this count costs nothing.

use std::{cell::Cell, rc::Rc};

use rquickjs::allocator::Allocator;

/// Bytes the runtime holds from the system, shared with the host.
#[derive(Debug, Clone, Default)]
pub(crate) struct HeapCount(Rc<Cell<usize>>);

impl HeapCount {
    pub(crate) fn bytes(&self) -> usize {
        self.0.get()
    }

    fn add(&self, bytes: usize) {
        self.0.set(self.0.get().saturating_add(bytes));
    }

    fn sub(&self, bytes: usize) {
        self.0.set(self.0.get().saturating_sub(bytes));
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
