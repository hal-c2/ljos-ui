//! The tier-2 JS host (ARCHITECTURE.md sections 3.2, 5.4, 8.5, 9.2, 9.3).
//!
//! One [`JsHost`] owns one QuickJS `Runtime` and `Context` and lives on the
//! UI thread (it is `!Send`). Everything that runs JS goes through
//! [`JsHost::enter`], which arms the interrupt deadline and maps every way a
//! run can fail onto a [`JsFault`]. The crate does not know about the Engine:
//! the Engine plugs in through the [`HostObjects`] trait.
//!
//! # Module format
//!
//! Each compiled QML file is one ES module whose exported functions take their
//! resolved references as parameters. We declare and evaluate it with
//! `Module::declare`, then walk the module namespace and keep every exported
//! function as a `Persistent<Function>` in a slotmap, addressed by [`FnId`].
//! Modules cannot `import` yet (no loader is installed); that arrives with
//! `.pragma library` scripts. QuickJS has no module-unload API, so
//! [`JsHost::unload_module`] only drops our handles.
//!
//! # Re-entrancy
//!
//! rquickjs holds a runtime lock for the duration of `Context::with`, and a
//! nested `with` panics. Host callbacks receive a [`Scope`] that lets them
//! call back into JS on the context that is already entered, and a nested
//! [`JsHost::enter`] reuses that context too: a lazy binding pulled from
//! inside JS runs its own JS under the outer run's deadline (ARCHITECTURE.md
//! section 6.1). Only the operations that must own the lock (`pump_jobs`,
//! collection, memory statistics) refuse to run inside a run.

mod bytecode;
mod color;
mod config;
mod fault;
mod host;
mod objects;
mod value;

pub use bytecode::{Bytecode, BytecodeOptions};
pub use config::{JsConfig, Slice};
pub use fault::{JsFault, SourceMapper};
pub use host::{FnId, HostFn, JsHost, MemoryStats, ModuleHandle, Scope};
pub use objects::{HostError, HostObjects, HostTypeId, MethodDef, PropDef, SignalDef, TypeDef};
pub use value::{JsArg, JsOut, JsRef, JsValue, ValueKind};

/// Re-export so callers write closures against the exact rquickjs we link.
pub use rquickjs;
