//! Runtime core of Ljós: how work reaches the UI thread and when it runs.
//!
//! Nothing here depends on egui, the engine or a JS runtime. The engine
//! instantiates the generic pieces ([`UiInbox<M>`], [`Scheduler<C>`]) with its
//! own message and context types.
//!
//! | Module | Role | ARCHITECTURE.md |
//! |---|---|---|
//! | [`inbox`] | bounded, coalescing `UiInbox`/`UiHandle` | 3.3 |
//! | [`time`] | `Time`, `Clock`, `SystemClock`, `VirtualClock` | 8.1 |
//! | [`frame_scheduler`] | next wake time, timers, 0 % idle | 8.1 |
//! | [`lane`], [`budget`], [`job`], [`scheduler`] | lanes, slices, jobs, `FrameReport` | 9.1 |
//! | [`fault`], [`supervisor`] | `Fault`, restart policy and intensity, the supervisor tree, supervised tokio tasks (feature `tasks`) | 9.3, 9.4 |
//! | [`pool`] | `ComputePool`: bounded worker threads for decode and parse | 3.3 |
//! | [`memory`] | `MemoryPressure`, `Evictable`, byte budgets, `ByteLru` | 8.5 |
//!
//! Nothing in this crate polls or sleeps: the only things that wake the UI
//! thread are the shell's event loop and the inbox's wake hook.

pub mod budget;
pub mod fault;
pub mod frame_scheduler;
pub mod inbox;
pub mod job;
pub mod lane;
pub mod memory;
pub mod pool;
pub mod scheduler;
pub mod supervisor;
pub mod time;

pub use budget::{FrameBudget, SliceBudget};
pub use fault::{Fault, JsErrorInfo};
pub use frame_scheduler::{
    CURSOR_BLINK_PERIOD, Due, DueTimer, FrameScheduler, MAX_TIMER_SLACK, TIMER_GRANULARITY,
    TimerId, TimerKind, WakeReasons,
};
pub use inbox::{
    CoalesceKey, DrainBudget, DrainReport, SendAsync, SendError, UiHandle, UiInbox, WakeHook,
};
pub use job::{FnJob, Job, JobCx, JobPoll};
pub use lane::{Lane, ProcId, ProcIdAllocator};
pub use memory::{
    ByteLru, DeviceClass, Evictable, MemoryPressure, image_cache_budget, physical_memory,
    usable_memory, usable_memory_under,
};
pub use pool::{ComputePool, PoolFull};
pub use scheduler::{FrameReport, Scheduler};
pub use supervisor::{
    APP_INTENSITY, ChildInfo, ChildKind, ChildSpec, Outcome, PLUGIN_INTENSITY, SERVICE_INTENSITY,
    StandardSups, StartError, SupId, SupervisorTree,
};
pub use supervisor::{
    ChildId, Directive, ExitReason, Intensity, IntensityTracker, IntensityVerdict, Restart,
    Strategy, Supervisor, SupervisorError,
};
pub use time::{Clock, SystemClock, Time, VirtualClock, parse_rfc3339_ms};
