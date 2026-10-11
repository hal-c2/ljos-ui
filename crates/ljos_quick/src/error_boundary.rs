//! `ErrorBoundary` (module `Ljos`): keeps a fault in one part of the tree
//! from being the whole application's (ARCHITECTURE §9.4).
//!
//! ```qml
//! import Ljos
//! ErrorBoundary {
//!     fallback: Text { text: "Something went wrong" }
//!     Page { }
//! }
//! ```
//!
//! What is written inside is a component, `content`; the boundary makes one
//! item of it. A fault below that item (a panic in a native type, a JS run
//! cut short, with `catchExceptions` an exception in a handler) stops the
//! content: the item is destroyed with everything it owns, which is how
//! whatever the fault left half done goes away. Then:
//!
//! - with a `fallback`, an item of that component takes the content's
//!   place and `fault` says what went wrong, until `retry()` makes the
//!   content again;
//! - without one the content is made again on the spot.
//!
//! Both count against the boundary's restart intensity (`maxRestarts`
//! within `within` milliseconds). A boundary that would exceed it gives
//! up: it stays failed and the fault is the next boundary's, or the
//! application's when there is none. The counting and the escalation are
//! the engine's supervisor tree ([`ljos_engine::Boundary`]); this file
//! only says how the content is stopped and started.
//!
//! The content is always made from the component, never repaired. State a
//! native type declares worth keeping (`ljos_engine::Migrate`) is taken
//! from the old item and given to the new one.

use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Atom, Boundary, BoundaryConfig, Cx, Engine, EngineError, FaultReport, LogLevel, ObjectId,
    PropIndex, QmlObject, QmlType, SubtreeSnapshot, Url, Value, WriteKind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, Restart};

use crate::item::{Item, is_item, size_follows_implicit, write_computed};
use crate::tree;

/// `ErrorBoundary` (Ljos).
#[derive(Debug, QmlObject)]
#[qml(module = "Ljos", name = "ErrorBoundary", extends = Item, default = "content")]
pub struct ErrorBoundary {
    #[qml(base)]
    pub item_base: Item,
    /// What the boundary shows and answers for.
    #[qml(notify, object = "Component")]
    pub content: Option<ObjectId>,
    /// What it shows while the content is stopped.
    #[qml(notify, object = "Component")]
    pub fallback: Option<ObjectId>,
    /// The item the boundary shows: the one made of `content`, or of
    /// `fallback` while the boundary has failed.
    #[qml(notify, readonly)]
    pub item: Option<ObjectId>,
    /// The content is stopped: the fallback is shown, or the boundary gave
    /// up.
    #[qml(notify, readonly)]
    pub failed: bool,
    #[qml(group)]
    pub fault: BoundaryFault,
    /// Whether an exception in a handler or a binding of the content is a
    /// fault. Off by default: Qt logs an exception and carries on, and so
    /// does a boundary that was not asked for more.
    #[qml(notify)]
    pub catch_exceptions: bool,
    /// More restarts than this within `within` and the boundary gives up.
    #[qml(notify)]
    pub max_restarts: i32,
    /// Milliseconds.
    #[qml(notify)]
    pub within: i32,
    /// How often the content was made again, by itself or by `retry()`.
    #[qml(notify, readonly)]
    pub restarts: i32,
    #[qml(skip)]
    run: Run,
}

impl Default for ErrorBoundary {
    fn default() -> Self {
        Self {
            item_base: Item::default(),
            content: None,
            fallback: None,
            item: None,
            failed: false,
            fault: BoundaryFault::default(),
            catch_exceptions: false,
            max_restarts: 3,
            within: 5000,
            restarts: 0,
            run: Run::default(),
        }
    }
}

/// The `fault` group of [`ErrorBoundary`]: the last fault of its content.
/// Empty while there was none and after `retry()`.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ErrorBoundaryFault", uncreatable)]
pub struct BoundaryFault {
    /// `"Panic"`, `"JsException"`, `"JsInterrupted"`, `"JsOutOfMemory"`,
    /// `"Startup"` or `"Internal"`.
    #[qml(notify, readonly)]
    pub kind: Atom,
    #[qml(notify, readonly)]
    pub message: Atom,
    /// The QML file the fault belongs to.
    #[qml(notify, readonly)]
    pub file: Url,
    /// 1-based; 0 when unknown.
    #[qml(notify, readonly)]
    pub line: i32,
}

/// The part of a boundary that QML does not see.
#[derive(Debug, Default)]
struct Run {
    /// Nothing is made before the boundary is complete.
    completed: bool,
    /// The item of `fallback` that is shown.
    fallback_item: Option<ObjectId>,
    /// The fallback is being made: nothing the boundary owns is content.
    making_fallback: bool,
    /// What the stopped content's natives wanted kept.
    kept: Option<SubtreeSnapshot>,
}

/// The flattened indices of the `fault.*` leaves.
struct FaultProps {
    kind: PropIndex,
    message: PropIndex,
    file: PropIndex,
    line: PropIndex,
}

fn fault_props() -> &'static FaultProps {
    static PROPS: std::sync::OnceLock<FaultProps> = std::sync::OnceLock::new();
    PROPS.get_or_init(|| {
        let m = ErrorBoundary::static_meta();
        let p = |name: &str| {
            m.property_index(&format!("fault.{name}"))
                .expect("ErrorBoundary declares the fault group")
        };
        FaultProps {
            kind: p("kind"),
            message: p("message"),
            file: p("file"),
            line: p("line"),
        }
    })
}

#[qml_impl]
impl ErrorBoundary {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, ErrorBoundary::CONTENT.index, move |e, _| {
            // A failed boundary takes the new content at `retry()`.
            if !failed(e, me) {
                drop_item(e, me);
                start_content(e, me);
            }
        })?;
        e.effect_in(me, move |e| {
            for horizontal in [true, false] {
                if let Err(err) = resize(e, me, horizontal) {
                    warn(e, format!("ErrorBoundary: cannot size the item: {err}"));
                }
            }
        })?;
        with_run(e, me, |r| r.completed = true);
        start_content(e, me);
        Ok(())
    }

    /// Makes the content again after a fault. Nothing happens while the
    /// content runs.
    #[qml(invokable)]
    fn retry(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        if !failed(e, me) {
            return;
        }
        drop_item(e, me);
        publish(e, me, None);
        bump_restarts(e, me);
        start_content(e, me);
    }
}

/// Registers module `Ljos` and what makes an `ErrorBoundary` a boundary.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos" 1.0 => [ErrorBoundary]));
    engine.register_boundary::<ErrorBoundary>(Rc::new(Behaviour));
}

struct Behaviour;

impl Boundary for Behaviour {
    fn config(&self, e: &Engine, me: ObjectId) -> BoundaryConfig {
        let number = |p: PropIndex| e.read(me, p).to_number().max(0.0);
        BoundaryConfig {
            restart: Restart::Permanent,
            intensity: Intensity::new(
                number(ErrorBoundary::MAX_RESTARTS.index) as u32,
                Duration::from_millis(number(ErrorBoundary::WITHIN.index) as u64),
            ),
            catch_exceptions: e
                .read(me, ErrorBoundary::CATCH_EXCEPTIONS.index)
                .to_boolean(),
        }
    }

    fn guards(&self, e: &Engine, me: ObjectId, child: ObjectId) -> bool {
        // A fault in the fallback is not the boundary's own: showing the
        // fallback for it would only fault again.
        e.with(me, |b: &ErrorBoundary| {
            !b.run.making_fallback && b.run.fallback_item != Some(child)
        })
        .unwrap_or(false)
    }

    fn stop(&self, e: &Engine, me: ObjectId, report: &FaultReport) {
        let item = e.read(me, ErrorBoundary::ITEM.index).as_object();
        let is_fallback = with_run(e, me, |r| r.fallback_item.is_some()) == Some(true);
        // A fallback that is still up was shown for an earlier fault and
        // has nothing worth keeping.
        if let Some(item) = item.filter(|i| e.is_alive(*i) && !is_fallback) {
            let kept = e.snapshot_subtree(item);
            with_run(e, me, |r| r.kept = kept);
        }
        drop_item(e, me);
        publish(e, me, Some(report));
        set(e, me, ErrorBoundary::FAILED.index, true.into());
    }

    fn start(&self, e: &Engine, me: ObjectId) -> Result<(), Fault> {
        let fallback = e
            .read(me, ErrorBoundary::FALLBACK.index)
            .as_object()
            .filter(|f| e.is_alive(*f));
        let Some(fallback) = fallback else {
            bump_restarts(e, me);
            return make_content(e, me).map_err(Fault::Startup);
        };
        with_run(e, me, |r| r.making_fallback = true);
        let made = e.component_create_owned(fallback, me, place);
        with_run(e, me, |r| r.making_fallback = false);
        match made {
            Ok(obj) if e.is_alive(me) => {
                with_run(e, me, |r| r.fallback_item = Some(obj));
                set(e, me, ErrorBoundary::ITEM.index, Value::Object(obj));
                Ok(())
            }
            Ok(_) => Ok(()),
            // With nothing to show in the content's place the boundary
            // cannot contain the fault.
            Err(err) => Err(Fault::Startup(format!("fallback: {err}"))),
        }
    }
}

fn with_run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut(me, |b: &mut ErrorBoundary| f(&mut b.run)).ok()
}

fn warn(e: &Engine, message: String) {
    e.log(LogLevel::Warning, "qml", message);
}

fn failed(e: &Engine, me: ObjectId) -> bool {
    e.untrack(|| e.read(me, ErrorBoundary::FAILED.index).to_boolean())
}

/// Writes one of the boundary's own properties; a change runs its handlers
/// before this returns.
fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        warn(e, format!("ErrorBoundary: {err}"));
    }
}

fn bump_restarts(e: &Engine, me: ObjectId) {
    let n = e.untrack(|| e.read(me, ErrorBoundary::RESTARTS.index).to_number()) as i32;
    set(
        e,
        me,
        ErrorBoundary::RESTARTS.index,
        n.saturating_add(1).into(),
    );
}

/// Shows `report` in `fault`, or nothing.
fn publish(e: &Engine, me: ObjectId, report: Option<&FaultReport>) {
    let p = fault_props();
    let (kind, message, file, line) = match report {
        Some(r) => (
            r.kind(),
            r.message(),
            r.file.clone().unwrap_or_default(),
            r.line,
        ),
        None => ("", String::new(), String::new(), 0),
    };
    e.batch(|| {
        set(e, me, p.kind, kind.into());
        set(e, me, p.message, message.as_str().into());
        set(e, me, p.file, Value::Url(Atom::from(file.as_str())));
        set(
            e,
            me,
            p.line,
            i32::try_from(line).unwrap_or(i32::MAX).into(),
        );
    });
}

/// Makes the content, or reports why it could not be made: that is a
/// fault of the boundary like one in the content.
fn start_content(e: &Engine, me: ObjectId) {
    match make_content(e, me) {
        Ok(()) => set(e, me, ErrorBoundary::FAILED.index, false.into()),
        Err(message) => e.report_boundary_fault(me, Fault::Startup(message)),
    }
}

/// Makes an item of `content` and shows it.
fn make_content(e: &Engine, me: ObjectId) -> Result<(), String> {
    if !e.is_alive(me) || with_run(e, me, |r| r.completed) != Some(true) {
        return Ok(());
    }
    let content = e
        .untrack(|| e.read(me, ErrorBoundary::CONTENT.index))
        .as_object()
        .filter(|c| e.is_alive(*c));
    let Some(content) = content else {
        // A boundary around nothing has nothing that can fail.
        set(e, me, ErrorBoundary::FAILED.index, false.into());
        return Ok(());
    };
    let made = e
        .component_create_owned(content, me, place)
        .map_err(|err| err.to_string())?;
    if !e.is_alive(me) {
        return Ok(());
    }
    if let Some(kept) = with_run(e, me, |r| r.kept.take()).flatten() {
        e.restore_subtree(made, kept);
    }
    set(e, me, ErrorBoundary::ITEM.index, Value::Object(made));
    set(e, me, ErrorBoundary::FAILED.index, false.into());
    Ok(())
}

/// Puts a new item in the boundary before its bindings run, so `parent`
/// reads right in them.
fn place(e: &Engine, obj: ObjectId) {
    let me = e.owner(obj);
    if is_item(e, obj) {
        let _ = tree::set_parent_item(e, obj, me);
    }
}

/// Takes away what the boundary shows, the content's item or the
/// fallback's.
fn drop_item(e: &Engine, me: ObjectId) {
    let item = e
        .untrack(|| e.read(me, ErrorBoundary::ITEM.index))
        .as_object();
    with_run(e, me, |r| r.fallback_item = None);
    set(e, me, ErrorBoundary::ITEM.index, Value::Null);
    if let Some(item) = item {
        discard(e, item);
    }
}

/// Takes `obj` off the boundary now and destroys it at the next frame: a
/// handler of its own may be what is running (a button of the fallback
/// calling `retry()`).
fn discard(e: &Engine, obj: ObjectId) {
    if !e.is_alive(obj) {
        return;
    }
    e.announce_destruction(obj);
    if is_item(e, obj) {
        let _ = tree::set_parent_item(e, obj, None);
    }
    e.destroy(obj);
}

/// Keeps the boundary's size and that of what it shows together along one
/// axis, the way a Loader does: a boundary given no size takes the item's,
/// one with a size of its own gives it to the item.
fn resize(e: &Engine, me: ObjectId, horizontal: bool) -> Result<(), EngineError> {
    let (size, implicit) = if horizontal {
        (Item::WIDTH.index, Item::IMPLICIT_WIDTH.index)
    } else {
        (Item::HEIGHT.index, Item::IMPLICIT_HEIGHT.index)
    };
    let own = e.read(me, size).to_number();
    let follows = size_follows_implicit(e, me, horizontal);
    let shown = e.read(me, ErrorBoundary::ITEM.index).as_object();
    let Some(shown) = shown.filter(|s| e.is_alive(*s) && is_item(e, *s)) else {
        return write_computed(e, me, implicit, 0.0.into());
    };
    if follows {
        let v = e.read(shown, size);
        return write_computed(e, me, implicit, v);
    }
    if e.untrack(|| e.read(shown, size)).to_number() != own {
        e.write_internal(shown, size, own.into(), WriteKind::Imperative)?;
    }
    let v = e.read(shown, implicit);
    write_computed(e, me, implicit, v)
}
