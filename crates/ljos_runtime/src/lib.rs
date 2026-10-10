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
//! | [`fault`], [`supervisor`] | `Fault`, restart policy and intensity | 9.3, 9.4 |
//!
//! Nothing in this crate polls or sleeps: the only things that wake the UI
//! thread are the shell's event loop and the inbox's wake hook.

pub mod budget;
pub mod fault;
pub mod frame_scheduler;
pub mod inbox;
pub mod job;
pub mod lane;
pub mod scheduler;
pub mod supervisor;
pub mod time;

pub use budget::{FrameBudget, SliceBudget};
pub use fault::{Fault, JsErrorInfo};
pub use frame_scheduler::{
    CURSOR_BLINK_PERIOD, Due, DueTimer, FrameScheduler, MAX_TIMER_SLACK, TIMER_GRANULARITY,
    TimerId, TimerKind, WakeReasons,
};
pub use inbox::{CoalesceKey, DrainBudget, DrainReport, SendError, UiHandle, UiInbox, WakeHook};
pub use job::{FnJob, Job, JobCx, JobPoll};
pub use lane::{Lane, ProcId, ProcIdAllocator};
pub use scheduler::{FrameReport, Scheduler};
pub use supervisor::{
    ChildId, Directive, ExitReason, Intensity, IntensityTracker, IntensityVerdict, Restart,
    Strategy, Supervisor, SupervisorError,
};
pub use time::{Clock, SystemClock, Time, VirtualClock};
