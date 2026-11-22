use std::time::Duration;

/// Limits for one JS runtime. Defaults are the section 8.5 budget.
#[derive(Debug, Clone)]
pub struct JsConfig {
    /// `set_memory_limit`. `None` means unlimited (tests, tools). The
    /// default is [`default_memory_limit`].
    pub memory_limit: Option<usize>,
    /// QuickJS runs its cycle collector after the heap grows by this much.
    pub gc_threshold: usize,
    /// Native stack the interpreter may use below where the host is made
    /// before throwing a RangeError, at most. `None`: the thread's stack
    /// down to a reserve kept for native code, since Rust frames between
    /// nested runs count too and a fixed size runs out in deep creation;
    /// [`FALLBACK_MAX_STACK`] on a platform whose thread stack bounds Ljós
    /// cannot read. A size given is kept to the room down to that reserve,
    /// and to [`FALLBACK_MAX_STACK`] where the bounds are unknown.
    pub max_stack: Option<usize>,
    /// Default per-dispatch budget used by [`crate::JsHost::call`].
    pub slice: Slice,
    /// Budget for compiling and evaluating a module or registering a type.
    /// Larger than `slice` because a 200 KB module takes ~8 ms to compile.
    pub load_slice: Slice,
    /// [`crate::JsHost::collect_if_idle`] only collects once the heap grew by
    /// at least this much since the last collection, so an idle frame with
    /// nothing allocated costs nothing.
    pub idle_collect_min_growth: usize,
    /// How much of the room left under the heap's limit
    /// ([`JsConfig::memory_limit`], or the one
    /// [`crate::JsHost::set_memory_limit`] put since) when a dispatch
    /// begins it may take, between 0 and 1: past that the
    /// dispatch is refused room, as at the limit, and told as the heap
    /// running out ([`crate::JsFault::JsOutOfMemory`]); what is left stays for
    /// the rest of the application. One script that keeps allocating then
    /// fails alone instead of filling the heap for every other. Each
    /// promise job [`crate::JsHost::pump_jobs`] runs is a dispatch of its
    /// own, with its own share; a run nested in a dispatch shares its cap.
    /// The share covers what scripts allocate and what they cause to be
    /// made, the registrations of types declared in QML or made from names
    /// a script gives among them. It leaves out one thing: the registration
    /// of a type of the engine's own ([`crate::TypeDef::engine_owned`]),
    /// which any dispatch may be the first to need and which happens once.
    /// That is held to the heap's limit only, and the dispatch has as much
    /// of its share left after it as before. One that fails is back under
    /// the share from its first error, before making that error can run
    /// script, and what it left is the dispatch's.
    /// A loop that awaits between allocations is a job per turn, so a
    /// share does not stop it growing the heap over many turns.
    /// `None` lets a dispatch take all the room there is.
    pub dispatch_heap_share: Option<f32>,
}

/// The heap a JS runtime may hold by default: a thirty-second of the
/// memory this process may use (`ljos_runtime::usable_memory`), at least
/// 96 MiB and at most 512 MiB; 96 MiB where the platform does not say.
/// A phone of 3 GB gets 96 MiB, a desktop of 16 GB 512 MiB. One fixed
/// size for both either starves a desktop app that keeps a lot in
/// script, as HAL-C2's sidebar did at 96 MiB, or lets a phone's app grow
/// until the system kills it.
pub fn default_memory_limit() -> usize {
    const LEAST: usize = 96 * 1024 * 1024;
    const MOST: usize = 512 * 1024 * 1024;
    ljos_runtime::usable_memory().map_or(LEAST, |m| {
        usize::try_from(m / 32).unwrap_or(MOST).clamp(LEAST, MOST)
    })
}

/// The stack JS may use where the thread's stack bounds are unknown and
/// [`JsConfig::max_stack`] gives none.
pub const FALLBACK_MAX_STACK: usize = 512 * 1024;

impl Default for JsConfig {
    fn default() -> Self {
        Self {
            memory_limit: Some(default_memory_limit()),
            gc_threshold: 8 * 1024 * 1024,
            max_stack: None,
            slice: Slice::default(),
            load_slice: Slice {
                soft: Duration::from_millis(50),
                hard: Duration::from_secs(2),
            },
            idle_collect_min_growth: 256 * 1024,
            // A quarter of the room is left for the rest. QuickJS collects
            // once the heap passes 1.5 times what survived its last
            // collection, so where more than some 60 % of the limit
            // survived, a dispatch that makes garbage in cycles may be
            // refused room before a collection runs, and is told as out of
            // memory as it would be at the limit itself.
            dispatch_heap_share: Some(0.75),
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
            // The interrupt is for runaway scripts. A debug build runs
            // them several times slower (as the soft warning allows for in
            // `JsHost::enter`), and cutting off there what a user's build
            // finishes leaves half a scene and a misleading warning.
            hard: Duration::from_millis(if cfg!(debug_assertions) { 800 } else { 100 }),
        }
    }
}
