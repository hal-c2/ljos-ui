//! A host calls `Inspector::observe` after every frame, so while a reload
//! error stands it must cost a comparison, not a copy of the error and its
//! text each time. A binary of its own: the allocator counts this thread.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

use ljos_dev::mcp::{Inspector, InspectorConfig};
use ljos_engine::{Engine, ReloadFault};

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

// SAFETY: every call is the system allocator's, with the same arguments.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
        // SAFETY: as the caller promises for `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as the caller promises for `dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn a_standing_reload_error_is_not_copied_at_every_look() {
    let engine = Engine::new();
    let (mut inspector, _surface) = Inspector::new(InspectorConfig::default(), Arc::new(|| {}));
    inspector.observe(&engine);
    engine.set_last_reload_error(Some(ReloadFault {
        file: "Main.qml".into(),
        line: 3,
        column: 5,
        message: "expected `}`".into(),
    }));
    // The first look after the failure tells clients of it.
    inspector.observe(&engine);

    let before = allocations();
    for _ in 0..100 {
        inspector.observe(&engine);
    }
    assert_eq!(allocations() - before, 0, "allocations in 100 looks");
}
