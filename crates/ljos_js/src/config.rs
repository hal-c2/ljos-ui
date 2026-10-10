use std::time::Duration;

/// Limits for one JS runtime. Defaults are the section 8.5 budget.
#[derive(Debug, Clone)]
pub struct JsConfig {
    /// `set_memory_limit`. `None` means unlimited (tests, tools).
    pub memory_limit: Option<usize>,
    /// QuickJS runs its cycle collector after the heap grows by this much.
    pub gc_threshold: usize,
    /// Native stack the interpreter may use before throwing a RangeError.
    pub max_stack: usize,
    /// Default per-dispatch budget used by [`crate::JsHost::call`].
    pub slice: Slice,
    /// Budget for compiling and evaluating a module or registering a type.
    /// Larger than `slice` because a 200 KB module takes ~8 ms to compile.
    pub load_slice: Slice,
    /// [`crate::JsHost::collect_if_idle`] only collects once the heap grew by
    /// at least this much since the last collection, so an idle frame with
    /// nothing allocated costs nothing.
    pub idle_collect_min_growth: usize,
}

impl Default for JsConfig {
    fn default() -> Self {
        Self {
            memory_limit: Some(96 * 1024 * 1024),
            gc_threshold: 8 * 1024 * 1024,
            max_stack: 512 * 1024,
            slice: Slice::default(),
            load_slice: Slice {
                soft: Duration::from_millis(50),
                hard: Duration::from_secs(2),
            },
            idle_collect_min_growth: 256 * 1024,
        }
    }
}

/// Time budget for one dispatch into JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    /// Overrunning this is only logged.
    pub soft: Duration,
    /// Overrunning this interrupts the script (uncatchable from JS).
    pub hard: Duration,
}

impl Default for Slice {
    fn default() -> Self {
        Self {
            soft: Duration::from_millis(4),
            hard: Duration::from_millis(100),
        }
    }
}
