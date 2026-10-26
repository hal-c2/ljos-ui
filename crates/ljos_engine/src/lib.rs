//! `ljos_engine`: the engine core (ARCHITECTURE.md §6).
//!
//! - [`Value`], the 16-byte dynamic value, with ECMAScript coercions;
//!   strings are [`Atom`]s.
//! - [`MetaObject`], the type description, built by
//!   `#[derive(QmlObject)]` / `#[qml_impl]` for native types and by
//!   [`MetaBuilder`] for types declared in QML.
//! - [`Engine`]: the object store, reactive properties on
//!   [`ljos_reactive::Graph`], signals, contexts and the type registry.
//!
//! The engine is a runtime crate: it knows nothing of QML source. The
//! compiler talks to it through `ljos_ir`, natives through the macros.
//!
//! ```
//! use ljos_engine::{Engine, QmlObject, qml_impl};
//!
//! #[derive(Default, QmlObject)]
//! #[qml(module = "Example", name = "Counter")]
//! pub struct Counter {
//!     #[qml(notify)]
//!     count: i32,
//! }
//!
//! #[qml_impl]
//! impl Counter {
//!     #[qml(invokable)]
//!     fn increment(cx: &ljos_engine::Cx) -> Result<(), ljos_engine::EngineError> {
//!         let n = cx.get(Counter::COUNT)?;
//!         cx.set(Counter::COUNT, n + 1)
//!     }
//! }
//!
//! let engine = Engine::new();
//! engine.register_module(ljos_engine::module!("Example" 1.0 => [Counter]));
//! let c = engine.create_by_name("Example", "Counter", None).unwrap();
//! engine.invoke_by_name(c, "increment", &[]).unwrap();
//! assert_eq!(engine.get(c, Counter::COUNT), Ok(1));
//! ```

// Lets the derive's `::ljos_engine::…` paths work inside this crate too.
extern crate self as ljos_engine;

mod atom;
mod boundary;
mod builtins;
mod chooser;
mod components;
mod context;
mod cx;
mod delayed_errors;
mod delegates;
mod describe;
mod devtools;
mod diagnostic_url;
pub use diagnostic_url::diagnostic_url;
mod engine;
mod error;
mod fault;
mod foreign;
mod freeze;
mod hooks;
mod i18n;
mod ids;
mod incubate;
mod incubator;
mod instantiator;
mod introspect;
mod item_selection;
mod js;
mod list;
mod list_faces;
mod listmodel;
mod loader;
mod log;
mod lookups;
mod made_on_read;
pub use made_on_read::BindOnRead;
mod journal;
mod made_when_reached;
mod member_aliases;
mod memory;
mod meta;
mod model;
mod number;
mod number_slot;
mod object;
mod plan;
mod profile;
mod property_map;
mod qml_types;
mod qt_enums;
mod qt_keys;
mod qtqml;
mod records;
mod registry;
mod reset;
mod script_arrays;
mod script_owned;
mod signal;
mod slow;
mod sort_filter;
mod synchronizer;
mod table_model;
mod timing;
mod trace;
mod tree;
mod tree_model;
mod types;
mod url_global;
mod value;
mod value_custom;
pub use value_custom::{CustomMember, CustomType, CustomValue};
mod value_faces;
mod value_types;
mod whole_group;

pub use atom::Atom;
pub use boundary::{APPLICATION_PROC, Boundary, BoundaryConfig, SubtreeSnapshot, SupervisorInfo};
pub use builtins::QtObject;
pub use chooser::{DelegateChoice, DelegateChooser};
pub use components::{ComponentKind, LoadedComponent};
pub use context::Context;
pub use cx::Cx;
pub use delegates::{DelegateHost, Delegates, RemovalOrder};
pub use delegates::{Made, Urgency, ViewDelegates, ViewHost};
pub use describe::{QtNamespaceEnum, VALUE_TYPES, ValueType, qt_namespace};
pub use devtools::{BindingGraph, DependencyReport, EvalJson, MAX_EVAL_TIME};
pub use engine::{Engine, OverrideContent, ParentListener};
pub use error::{EngineError, JsError};
pub use fault::{FaultReport, StackFrame, TakenBy, fault_kind};
pub use foreign::{ForeignBinding, ForeignKeeper, ForeignMember, ForeignValue};
pub use hooks::{Interception, Interceptor};
pub use ids::{ConnectionId, ContextId, MethodIndex, ObjectId, PropIndex, PropKey, SignalIndex};
pub use incubate::Incubator;
pub use incubator::dynamic::{QmlText, QmlTextCompiler, QmlTextError};
pub use incubator::reload::{Migrate, ReloadError, ReloadFault, ReloadOutcome};
pub use incubator::singletons::{
    QmlSingleton, ScopedEvent, ScopedSingleton, SingletonScope, WindowScope,
};
pub use incubator::tier1::Tier1Stats;
pub use incubator::{CreationHandle, CreationJob, SharedCreation};
pub use instantiator::Instantiator;
pub use introspect::{BindingTier, ObjectReport, PropertyReport, SourcePlace};
pub use item_selection::ItemSelectionModel;
pub use journal::{Journal, JournalEntry, JournalKind, MAX_JOURNAL_RUNS};
pub use list::QmlList;
pub use listmodel::{CarriedCell, CarriedRows, ListElement, ListModel};
pub use ljos_js::{JsClock, Timing as JsTiming};
pub use loader::{ComponentSource, MapSource, encoded_url, file_url, url_path};
pub use log::{LogEntry, LogLevel};
pub use lookups::Lookups;
#[cfg(feature = "lookup-counts")]
pub use lookups::lookups;
pub use memory::{CompiledMemory, TypeMemory};
pub use meta::{
    AttachedTypeId, CompleteThunk, Declared, EnumInfo, Getter, GroupGet, GroupGetMut, ImplInfo,
    ListAppend, ListField, ListInsert, ListRemove, MetaBuilder, MetaObject, MetaRef, MethodDecl,
    MethodInfo, MethodThunk, Prop, PropertyAccess, PropertyFlags, PropertyInfo, QmlImplInfo, Reset,
    ResetApply, ResetValue, Setter, SignalDecl, SignalInfo, WriteFilter,
};
pub use model::{DelegateGone, DelegateMade, DelegateRows, DelegateSource, ObjectRows};
pub use model::{ItemModel, ModelChange, ModelListener, ModelListeners, RowKey};
pub use model::{ModelSeen, ModelSignals, QmlListModel, list_model};
pub use number::{number_to_string, string_to_number, to_int32};
pub use number_slot::NumberSlot;
pub use object::{
    ObjectFlags, QmlObject, QmlType, ScriptMethod, downcast_chain, downcast_chain_mut, find_native,
    find_native_mut,
};
pub use profile::{BindingLoopSeen, HotCause, HotEntry, HotKind, HotRun, MAX_RUNS, Profile};
pub use property_map::PropertyMap;
pub use qtqml::{Component, ComponentAttached, Connections, RunningTimer, Timer, handler_name};
pub use registry::{
    EnumEntry, MethodEntry, ModuleDef, ModuleDump, ParamEntry, PropertyEntry, PropertyKind,
    Registry, SignalEntry, TypeDump, TypeEntry, Version,
};
pub use script_owned::ScriptMemory;
pub use signal::Handler;
pub use slow::SlowCall;
pub use sort_filter::{
    FilterBase, RoleFilter, RoleSorter, SortFilterProxyModel, SorterBase, StringSorter, ValueFilter,
};
pub use table_model::TableModel;
pub use timing::EngineTimes;
pub use trace::{TraceSink, Traced};
pub use tree::{ItemTree, TreeChange, TreeListener, TreeListeners, tree_model};
pub use tree_model::{TableModelColumn, TreeModel};
pub use types::{ConvertError, QmlValue, TypeTag, Url};
pub use value::{Color, EnumValue, Font, JsHandle, Point, Rect, Size, Unwritten, Value};
pub use value_types::{
    IntPoint, Quaternion, Vector2d, Vector3d, Vector4d, general as number_to_general,
};

pub use ljos_macros::{QmlObject, qml_impl, qml_singleton};
pub use ljos_reactive::{NodeId, OverrideId, Provenance, RestoreMode, ScopeId, WriteKind};

/// Paths for generated code. Not a public API.
#[doc(hidden)]
pub mod __private {
    pub use std::any::{Any, TypeId};
    pub use std::boxed::Box;
    pub use std::option::Option;
    pub use std::result::Result;
    pub use std::string::ToString;
    pub use std::sync::{LazyLock, OnceLock};
    pub use std::vec::Vec;

    pub use crate::meta::{ImplFallback, ImplViaTrait, Probe};

    /// Which of the two setter forms a `#[qml(write = ..)]` path is: the
    /// derive passes trampolines for both, and the method's signature
    /// picks one (the marker `M` is inferred).
    #[diagnostic::on_unimplemented(
        message = "a setter is `fn(&self, T) -> Option<T>` or `fn(cx: &Cx, T) -> Option<T>`, with `T` the field's type"
    )]
    pub trait WriteFn<S, T, M> {
        fn filter(&self, native: NativeWrite, cx: CxWrite) -> crate::WriteFilter;
        fn on_native(&self, this: &S, v: T) -> Option<T>;
        fn on_cx(&self, cx: &crate::Cx<'_>, v: T) -> Option<T>;
    }

    #[derive(Debug)]
    pub struct ByRef;
    #[derive(Debug)]
    pub struct ByCx;
    type NativeWrite =
        fn(&dyn QmlObject, &crate::Value) -> Result<Option<crate::Value>, crate::ConvertError>;
    type CxWrite = crate::meta::CxWrite;

    impl<S, T, F: Fn(&S, T) -> Option<T>> WriteFn<S, T, ByRef> for F {
        fn filter(&self, native: NativeWrite, _: CxWrite) -> crate::WriteFilter {
            crate::WriteFilter::Native(native)
        }
        fn on_native(&self, this: &S, v: T) -> Option<T> {
            self(this, v)
        }
        fn on_cx(&self, _: &crate::Cx<'_>, _: T) -> Option<T> {
            unreachable!("ljos_engine: a `&self` setter was given a Cx")
        }
    }

    impl<S, T, F: Fn(&crate::Cx<'_>, T) -> Option<T>> WriteFn<S, T, ByCx> for F {
        fn filter(&self, _: NativeWrite, cx: CxWrite) -> crate::WriteFilter {
            crate::WriteFilter::Cx(cx)
        }
        fn on_native(&self, _: &S, _: T) -> Option<T> {
            unreachable!("ljos_engine: a `Cx` setter was given the struct")
        }
        fn on_cx(&self, cx: &crate::Cx<'_>, v: T) -> Option<T> {
            self(cx, v)
        }
    }

    use crate::object::QmlObject;

    /// The struct a generated accessor was registered for. The engine has
    /// already matched the type id, so failure is a bug in the derive.
    pub fn downcast<T: QmlObject>(o: &dyn QmlObject) -> &T {
        o.as_any()
            .downcast_ref::<T>()
            .expect("ljos_engine: accessor called on the wrong native type")
    }

    pub fn downcast_mut<T: QmlObject>(o: &mut dyn QmlObject) -> &mut T {
        o.as_any_mut()
            .downcast_mut::<T>()
            .expect("ljos_engine: accessor called on the wrong native type")
    }

    /// The argument at `i`, `undefined` when missing (JS semantics).
    pub fn arg(args: &[crate::Value], i: usize) -> &crate::Value {
        static UNDEFINED: crate::Value = crate::Value::Undefined;
        args.get(i).unwrap_or(&UNDEFINED)
    }
}
