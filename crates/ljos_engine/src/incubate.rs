//! `Component.incubateObject`: an object made in slices, and the incubator
//! that says how far it is (ARCHITECTURE.md §6.3).
//!
//! The incubator is what Qt documents: `status` (`Component.Loading`, then
//! `Ready` or `Error`), `object` once it is made, a function assigned to
//! `onStatusChanged` that is called with the new status, and
//! `forceCompletion()`, which makes what is left before it returns.
//!
//! It belongs to the script that holds it (script_owned.rs), as in Qt:
//! it outlives its component, and goes when the script lets go of it.
//! While its object is being made the engine holds it too, so an
//! incubator nobody kept still makes its object. `onStatusChanged` is no
//! property of the type: the script's function is kept by the incubator
//! as the script sees it, so a function that closes over its own
//! incubator (what Qt's documentation shows) is collected with it.
//!
//! An asynchronous incubation is a [`CreationJob`] like a Loader's: queued
//! with [`Engine::incubate`], run by whoever runs the frame, a slice at a
//! time, charged to the boundary the parent is in. Nothing is scheduled
//! once it is done.

use std::cell::{Cell, RefCell};

use ljos_js::JsRef;
use ljos_runtime::Lane;

use crate::atom::Atom;
use crate::boundary::APPLICATION_PROC;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::incubator::{CreationHandle, CreationJob, Initial, SharedCreation};
use crate::js::obj_to_u64;
use crate::value::Value;
use crate::{QmlObject, TypeTag, WriteKind, qml_impl};

/// `Component.Status`, as far as an incubator has one.
mod status {
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
    pub const ERROR: i32 = 3;
}

/// `Qt.Synchronous`; anything else is asynchronous, the default.
const SYNCHRONOUS: f64 = 1.0;

/// What `incubateObject` returns. In no module: QML cannot name it.
#[derive(Default, QmlObject)]
#[qml(name = "ComponentIncubator", extends = QtObject, plain_object)]
pub struct Incubator {
    #[qml(base)]
    pub base: QtObject,
    #[qml(notify, readonly)]
    pub status: i32,
    #[qml(notify, readonly)]
    pub object: Option<ObjectId>,
    #[qml(skip)]
    job: Option<(SharedCreation, CreationHandle)>,
    /// The incubator as scripts see it, held while its object is being
    /// made: what keeps it then, and where `onStatusChanged` is.
    #[qml(skip)]
    face: Option<JsRef>,
}

/// What a script assigns the function to that is called with the status
/// when it changes.
const ON_STATUS_CHANGED: &str = "onStatusChanged";

impl std::fmt::Debug for Incubator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Incubator")
            .field("status", &self.status)
            .field("object", &self.object)
            .finish_non_exhaustive()
    }
}

#[qml_impl]
impl Incubator {
    /// Makes what is left of the object before it returns. Nothing
    /// happens when the incubation is done, or when it is the object
    /// being made that asks.
    #[qml(invokable, name = "forceCompletion")]
    fn force_completion(cx: &Cx) {
        let job = cx
            .engine
            .with(cx.this, |i: &Incubator| i.job.clone())
            .ok()
            .flatten();
        if let Some((job, _)) = job {
            job.force();
        }
    }
}

/// The incubations that are still running, for [`Engine::is_half_made`].
#[derive(Default)]
struct Running {
    handles: RefCell<Vec<CreationHandle>>,
    /// Made when somebody first watches ([`Engine::watch_incubations`]).
    ended: Cell<Option<ObjectId>>,
}

/// How many incubations have ended: something to read for an effect that
/// has to look again when one does. In no module.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "IncubationsEnded", extends = QtObject)]
struct Ended {
    #[qml(base)]
    base: QtObject,
    #[qml(notify, readonly)]
    count: i32,
}

impl Engine {
    /// `component.incubateObject(parent, properties, mode)`: the
    /// incubator.
    pub(crate) fn incubate_object(
        &self,
        component: ObjectId,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
        synchronous: bool,
    ) -> Result<ObjectId, EngineError> {
        let inc = self.create_native_incomplete(Incubator::default(), None)?;
        self.give_to_script(inc);
        self.write_internal(
            inc,
            Incubator::STATUS.index,
            status::LOADING.into(),
            WriteKind::Initial,
        )?;
        // What cannot be set is told at the call, which an asynchronous
        // incubation is past by then. Nothing to set, nothing to tell: no
        // stack is taken.
        let at = if props.is_empty() {
            None
        } else {
            self.js_origin()
                .map(|p| Atom::from(format!("{}:{}", p.file, p.line)))
        };
        let how = Initial::Incubated(at);
        if synchronous {
            let made = self.component_template(component).and_then(|template| {
                self.create_for_script(component, template, parent, props, how)
            });
            self.incubated(inc, &made);
            return Ok(inc);
        }
        let owner = parent.map_or(APPLICATION_PROC, |p| self.proc_for(p));
        let (job, handle) =
            CreationJob::for_component(self, component, parent, props, Lane::Visible, owner);
        let (job, shared) = job
            .initial(how)
            .on_done(move |e, made| e.incubated(inc, made))
            .shared();
        self.extension::<Running>()
            .handles
            .borrow_mut()
            .push(handle.clone());
        let face = self.run_js("incubator", |s| {
            Ok(s.keep(s.wrapper(obj_to_u64(inc))?.into_value()))
        });
        self.with_mut(inc, |i: &mut Incubator| {
            i.job = Some((shared, handle));
            i.face = face;
        })?;
        self.incubate(job);
        Ok(inc)
    }

    /// Publishes what an incubation made, or that it could not.
    fn incubated(&self, inc: ObjectId, made: &Result<ObjectId, EngineError>) {
        self.extension::<Running>()
            .handles
            .borrow_mut()
            .retain(|h| !h.is_done());
        // Whoever left the half-made object out looks again.
        if let Some(ended) = self.extension::<Running>().ended.get() {
            let n = self.untrack(|| self.read(ended, Ended::COUNT.index).to_number()) as i32;
            let _ = self.write_internal(
                ended,
                Ended::COUNT.index,
                n.wrapping_add(1).into(),
                WriteKind::Imperative,
            );
        }
        if !self.is_alive(inc) {
            // Nobody can reach what was made.
            if let Ok(obj) = made {
                self.destroy(*obj);
            }
            return;
        }
        // From here on only a script keeps the incubator.
        let face = self
            .with_mut(inc, |i: &mut Incubator| {
                i.job = None;
                i.face.take()
            })
            .ok()
            .flatten();
        let status = match made {
            Ok(obj) => {
                let _ = self.write_internal(
                    inc,
                    Incubator::OBJECT.index,
                    Value::Object(*obj),
                    WriteKind::Imperative,
                );
                status::READY
            }
            // Qt says nothing of a required property left out: the
            // incubator's status is all the script is told.
            Err(EngineError::RequiredNotSet(_)) => status::ERROR,
            Err(e) => {
                self.warn(format!("Component.incubateObject: {e}"));
                status::ERROR
            }
        };
        let _ = self.write_internal(
            inc,
            Incubator::STATUS.index,
            status.into(),
            WriteKind::Imperative,
        );
        let callback = face.and_then(|face| {
            self.run_js("incubator", |s| {
                let f = s.get_field(&face, ON_STATUS_CHANGED)?;
                Ok(self.js_to_value(s, f, &TypeTag::Var))
            })
        });
        if let Some(callback @ Value::Js(_)) = callback {
            let _origin = self.origin(Some(inc));
            self.call_js(&callback, &[status.into()]);
        }
    }

    /// Whether any incubation is running: the cheap question to ask before
    /// [`Engine::is_half_made`] of many objects.
    pub fn has_half_made(&self) -> bool {
        let running = self.extension::<Running>();
        let mut handles = running.handles.borrow_mut();
        // One called off ends without anybody being told.
        handles.retain(|h| !h.is_done());
        !handles.is_empty()
    }

    /// A tracked read of nothing but the end of incubations: an effect
    /// that left a half-made object out ([`Engine::is_half_made`]) calls
    /// this and runs again when one ends. Nothing is read while none
    /// runs, so nothing wakes then.
    pub fn watch_incubations(&self) {
        if !self.has_half_made() {
            return;
        }
        let running = self.extension::<Running>();
        let ended = match running.ended.get() {
            Some(o) => o,
            None => {
                let Ok(o) = self.untrack(|| self.create_native_incomplete(Ended::default(), None))
                else {
                    return;
                };
                running.ended.set(Some(o));
                o
            }
        };
        let _ = self.read(ended, Ended::COUNT.index);
    }

    /// Whether `obj` is the root of an incubation that is not done: it is
    /// in the tree already, half applied, and must not be shown.
    pub fn is_half_made(&self, obj: ObjectId) -> bool {
        let running = self.extension::<Running>();
        let handles = running.handles.borrow();
        !handles.is_empty() && handles.iter().any(|h| h.half_made() == Some(obj))
    }
}

/// `Component.incubateObject` as the method sees it.
pub(crate) fn incubate_object(cx: &Cx, parent: Value, props: Value, mode: Value) -> Value {
    let e = cx.engine;
    // A component a createObject left in Error makes nothing until one
    // succeeds.
    if e.component_failed(cx.this) {
        e.warn("QQmlComponent: Component is not ready".to_owned());
        return Value::Null;
    }
    let props = e.js_object_entries(&props);
    let synchronous =
        !matches!(mode, Value::Undefined | Value::Null) && mode.to_number() == SYNCHRONOUS;
    match e.incubate_object(cx.this, parent.as_object(), props, synchronous) {
        Ok(inc) => Value::Object(inc),
        Err(err) => {
            e.warn(format!("Component.incubateObject: {err}"));
            Value::Null
        }
    }
}
