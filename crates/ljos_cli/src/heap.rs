//! The `dhat-heap` build: every Rust heap allocation recorded with its
//! call stack. An application does not exit by itself, so the profile is
//! written after `LJOS_DHAT_SECONDS` (default 5) to `LJOS_DHAT_FILE`
//! (default `dhat-heap.json`) while it keeps running.

use std::{sync::Mutex, time::Duration};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

static PROFILER: Mutex<Option<dhat::Profiler>> = Mutex::new(None);

pub fn start() {
    let file = std::env::var("LJOS_DHAT_FILE").unwrap_or_else(|_| "dhat-heap.json".to_owned());
    let seconds = std::env::var("LJOS_DHAT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(5.0);
    let profiler = dhat::Profiler::builder().file_name(file).build();
    *PROFILER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(profiler);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f64(seconds));
        // Dropping the profiler writes the profile.
        drop(
            PROFILER
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take(),
        );
    });
}
