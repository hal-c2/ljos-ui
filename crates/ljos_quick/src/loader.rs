//! `Loader`: makes one item from a `Component` or a file, and drops or
//! replaces it when `active`, `source` or `sourceComponent` change.
//!
//! A plain Loader makes its item on the spot. An `asynchronous` one hands
//! a [`CreationJob`] to the engine ([`Engine::incubate`]); the scene's
//! incubation step runs it in slices, so a large component does not cost
//! a whole frame (ARCHITECTURE §6.3). While the job runs the half-made
//! root is already a child of the Loader, because `parent` must read
//! right inside it, but the scene leaves it out ([`shown_children`]).
//!
//! Sizes go the way Qt documents: a Loader that was given no size takes
//! its item's, and a Loader with a size of its own gives it to the item.
//!
//! An unloaded item is taken off the Loader at once and destroyed at the
//! next frame, since the handler that unloads it is often the item's own.
//!
//! A Loader is a UI boundary (ARCHITECTURE §3.4, §9.4): a fault below its
//! item (a panic in a native type, a JS run cut short) unloads the item
//! and leaves the Loader at `Loader.Error`, and the rest of the
//! application runs on. Loading again makes a new item from the source.
//! A JS run cut short may not happen again (what it did before may have
//! finished the work, or the data may have changed), so after one the Loader
//! loads again by itself, once in [`SECOND_CHANCE`], and the log says so.
//! An exception in a handler is not such a fault: Qt logs it and keeps
//! the item, and so does this. The work of making the item is charged to
//! the Loader as its owner.

use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Atom, Boundary, BoundaryConfig, CreationHandle, CreationJob, Cx, Engine, EngineError,
    FaultReport, Interception, LogLevel, ObjectId, PropIndex, QmlObject, Url, Value, WriteKind,
    qml_impl,
};
use ljos_runtime::{Fault, Intensity, Lane, Restart, Time};

use crate::enums::loader_status;
use crate::item::{Item, is_a, is_item, size_follows_implicit, write_computed};
use crate::tree;

/// How long after loading again for an interrupted script a Loader takes
/// another such fault as final.
const SECOND_CHANCE: Duration = Duration::from_secs(10);

/// `Loader` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Loader",
    extends = Item,
    enums = crate::enums::loader_enums
)]
pub struct Loader {
    #[qml(base)]
    pub item_base: Item,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, reset, object = "Component")]
    pub source_component: Option<ObjectId>,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub asynchronous: bool,
    #[qml(notify, readonly, change = crate::scene::restructured)]
    pub item: Option<ObjectId>,
    /// [`crate::enums::loader_status`].
    #[qml(notify, readonly, enumeration = "Status", change = crate::scene::restructured)]
    pub status: i32,
    /// 0 while there is nothing to load or the item is being made, 1 once
    /// it is made or has failed.
    #[qml(notify, readonly)]
    pub progress: f64,
    #[qml(skip)]
    run: Run,
}

impl Default for Loader {
    fn default() -> Self {
        Self {
            item_base: Item::default(),
            source: Url::default(),
            source_component: None,
            active: true,
            asynchronous: false,
            item: None,
            status: loader_status::NULL,
            progress: 0.0,
            run: Run::default(),
        }
    }
}

/// What the item, or the job making it, was made from.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Origin {
    Url(Atom),
    Component(ObjectId),
}

/// The part of a Loader that QML does not see.
#[derive(Debug, Default)]
struct Run {
    /// Nothing loads before the Loader is complete.
    completed: bool,
    origin: Option<Origin>,
    job: Option<CreationHandle>,
    /// The half-made root of the running job.
    incubating: Option<ObjectId>,
    /// `setSource`'s property values and the source they belong to: they
    /// last until the source changes.
    props: Vec<(String, Value)>,
    props_for: Atom,
    /// Counts reloads, so a creation that comes back after another reload
    /// began knows its object is not wanted.
    generation: u64,
    /// The item whose `Component.onDestruction` is running.
    leaving: Option<ObjectId>,
    /// The `active` the Loader last followed, which a written one is
    /// followed before it is stored.
    active: Option<bool>,
    /// An item was emptied without telling while the next one is made: a
    /// failure tells the empty item, as no item replaces it.
    emptied_quietly: bool,
    /// When an interrupted script last made the Loader load again.
    second_chance: Option<Time>,
}

#[qml_impl]
impl Loader {
    #[qml(signal)]
    fn loaded();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, Loader::SOURCE.index, move |e, v| {
            let source = v.to_js_string();
            // A source and a component cannot both stand: the newer wins.
            if !source.is_empty() {
                set(e, me, Loader::SOURCE_COMPONENT.index, Value::Null);
            }
            with_run(e, me, |r| {
                if r.props_for != source {
                    r.props.clear();
                    r.props_for = Atom::default();
                }
            });
            reload(e, me, false);
        })?;
        e.watch(me, Loader::SOURCE_COMPONENT.index, move |e, v| {
            if v.as_object().is_some() {
                set(e, me, Loader::SOURCE.index, Value::Url(Atom::default()));
            }
            reload(e, me, false);
        })?;
        // A written or bound `active` is loaded or unloaded before its
        // change is told (qml6); the watch sees what else reaches it.
        e.set_interceptor(
            me,
            Loader::ACTIVE.index,
            Some(Rc::new(move |e, v, how| {
                let on = v.to_boolean();
                match how {
                    Interception::Write => {
                        turned(e, me, on);
                        // A handler run by turning it may have written a
                        // newer value; the outer write must not restore its
                        // stale one.
                        if with_run(e, me, |r| r.active).flatten() == Some(on) {
                            let _ = e.write_internal(
                                me,
                                Loader::ACTIVE.index,
                                on.into(),
                                WriteKind::Imperative,
                            );
                        }
                    }
                    // The binding stays; what it gave goes in once it has.
                    Interception::Binding => e.after_binding(move |e| {
                        turned(e, me, on);
                        let _ =
                            e.write_internal(me, Loader::ACTIVE.index, on.into(), WriteKind::Host);
                    }),
                }
                true
            })),
        );
        e.watch(me, Loader::ACTIVE.index, move |e, v| {
            turned(e, me, v.to_boolean())
        })?;
        e.watch(me, Loader::ASYNCHRONOUS.index, move |e, v| {
            // As Qt documents: switching it off while loading finishes the
            // load at once.
            let loading = with_run(e, me, |r| r.job.is_some()) == Some(true);
            if loading && !v.to_boolean() {
                reload(e, me, true);
            }
        })?;
        e.effect_in(me, move |e| {
            for horizontal in [true, false] {
                if let Err(err) = resize(e, me, horizontal) {
                    warn(e, format!("Loader: cannot size the item: {err}"));
                }
            }
        })?;
        let active = e.untrack(|| e.read(me, Loader::ACTIVE.index).to_boolean());
        with_run(e, me, |r| {
            r.completed = true;
            r.active = Some(active);
        });
        reload(e, me, false);
        Ok(())
    }

    /// `setSource(source, properties)`: loads `source` and gives the new
    /// item `properties` before it completes. The values are used again
    /// when the Loader is switched off and on.
    #[qml(invokable, name = "setSource")]
    fn set_source(cx: &Cx, source: Url, properties: Value) {
        let (e, me) = (cx.engine, cx.this);
        let props = e.js_object_entries(&properties);
        let url = source.0;
        with_run(e, me, |r| {
            r.props = props;
            r.props_for = url.clone();
        });
        // Qt reloads even when the source is the one already loaded.
        e.batch(|| {
            set(e, me, Loader::SOURCE_COMPONENT.index, Value::Null);
            set(e, me, Loader::SOURCE.index, Value::Url(url));
        });
        reload(e, me, true);
    }
}

fn with_run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut(me, |l: &mut Loader| f(&mut l.run)).ok()
}

fn warn(e: &Engine, message: String) {
    e.log(LogLevel::Warning, "qml", message);
}

/// Writes one of the Loader's own properties; a change runs its handlers
/// before this returns.
fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        warn(e, format!("Loader: {err}"));
    }
}

/// The visual children the scene shows: all of them, except a Loader's
/// item, or one `Component.incubateObject` makes, while it is still being
/// made.
pub(crate) fn shown_children(e: &Engine, obj: ObjectId) -> Vec<ObjectId> {
    let mut kids = tree::child_items(e, obj);
    if is_a::<Loader>(e, obj)
        && let Ok(Some(hidden)) = e.with(obj, |l: &Loader| l.run.incubating)
    {
        kids.retain(|c| *c != hidden);
    }
    if e.has_half_made() {
        kids.retain(|c| !e.is_half_made(*c));
    }
    kids
}

/// Makes every Loader a boundary.
pub(crate) fn register(engine: &Engine) {
    engine.register_boundary::<Loader>(Rc::new(LoaderBoundary));
}

struct LoaderBoundary;

impl Boundary for LoaderBoundary {
    fn config(&self, _: &Engine, _: ObjectId) -> BoundaryConfig {
        BoundaryConfig {
            // The supervisor restarts nothing: only an interrupted script
            // has the Loader load again (`stop`), and it counts those itself.
            restart: Restart::Temporary,
            // Nothing is restarted, so nothing is counted.
            intensity: Intensity::new(0, std::time::Duration::ZERO),
            // As in Qt, an exception is logged and the item stays.
            catch_exceptions: false,
        }
    }

    fn stop(&self, e: &Engine, me: ObjectId, report: &FaultReport) {
        let begun = with_run(e, me, |r| {
            // What a running job makes is not wanted any more.
            r.generation += 1;
            (r.job.take(), r.incubating.take())
        });
        let Some((job, half_made)) = begun else {
            return;
        };
        if let Some(job) = job {
            job.cancel();
        }
        if let Some(half) = half_made {
            discard(e, half);
        }
        let old = e.read(me, Loader::ITEM.index).as_object();
        set(e, me, Loader::ITEM.index, Value::Null);
        drop_old(e, me, old);
        set(e, me, Loader::STATUS.index, loader_status::ERROR.into());
        set(e, me, Loader::PROGRESS.index, 1.0.into());
        if matches!(report.fault, Fault::JsInterrupted { .. }) {
            load_again(e, me, report.at);
        }
    }

    fn start(&self, _: &Engine, _: ObjectId) -> Result<(), Fault> {
        // Never asked of a temporary child.
        Ok(())
    }

    fn failed(&self, e: &Engine, me: ObjectId) -> bool {
        e.untrack(|| e.read(me, Loader::STATUS.index).to_number())
            == f64::from(loader_status::ERROR)
    }
}

/// After a script was interrupted at `at`: loads again unless that was just
/// tried. The log has the fault already; this says what became of the item.
fn load_again(e: &Engine, me: ObjectId, at: Time) {
    let tried = with_run(e, me, |r| {
        let tried = r
            .second_chance
            .is_some_and(|t| at.saturating_since(t) < SECOND_CHANCE);
        if !tried {
            r.second_chance = Some(at);
        }
        tried
    });
    let place = e.declared_at(me).unwrap_or_default();
    match tried {
        Some(false) => {
            warn(
                e,
                format!("{place}Loader: a script was interrupted, loading again"),
            );
            reload(e, me, true);
        }
        Some(true) => warn(
            e,
            format!(
                "{place}Loader: a script was interrupted again, so it stays at Loader.Error until it is told to load"
            ),
        ),
        None => {}
    }
}

/// Called when `child` is appended to `parent`'s `data`: a Loader with a
/// running job learns which child the job is making.
pub(crate) fn note_child(e: &Engine, parent: ObjectId, child: ObjectId) {
    if is_a::<Loader>(e, parent) {
        with_run(e, parent, |r| {
            if r.job.as_ref().is_some_and(|j| !j.is_done()) && r.incubating.is_none() {
                r.incubating = Some(child);
            }
        });
    }
}

/// Follows `active` turned `on`, once for each change.
fn turned(e: &Engine, me: ObjectId, on: bool) {
    let was = with_run(e, me, |r| r.active.replace(on)).flatten();
    if was.is_none_or(|w| w == on) {
        return;
    }
    reload(e, me, false);
    // Turned on with nothing to load, it still tells that its progress
    // and its item changed (qml6).
    if on && with_run(e, me, |r| r.origin.is_none()) == Some(true) {
        let progress = e.untrack(|| e.read(me, Loader::PROGRESS.index).to_number());
        if progress == 0.0 {
            let _ = e.notify(me, Loader::PROGRESS.index);
        } else {
            set(e, me, Loader::PROGRESS.index, 0.0.into());
        }
        let _ = e.notify(me, Loader::ITEM.index);
    }
}

/// Brings the Loader in step with `active`, `source` and
/// `sourceComponent`. `force` loads again even when the origin is the one
/// already loaded.
fn reload(e: &Engine, me: ObjectId, force: bool) {
    if !e.is_alive(me) {
        return;
    }
    let (active, component, source, asynchronous) = e.untrack(|| {
        (
            with_run(e, me, |r| r.active)
                .flatten()
                .unwrap_or_else(|| e.read(me, Loader::ACTIVE.index).to_boolean()),
            e.read(me, Loader::SOURCE_COMPONENT.index).as_object(),
            e.read(me, Loader::SOURCE.index).to_js_string(),
            e.read(me, Loader::ASYNCHRONOUS.index).to_boolean(),
        )
    });
    let named = component.is_some() || !source.is_empty();
    let want = if !active {
        None
    } else if let Some(c) = component {
        Some(Origin::Component(c))
    } else if !source.is_empty() {
        Some(Origin::Url(source))
    } else {
        None
    };
    let begun = with_run(e, me, |r| {
        if !r.completed || (!force && r.origin == want) {
            return None;
        }
        r.generation += 1;
        r.origin = want.clone();
        Some((r.generation, r.job.take(), r.incubating.take()))
    })
    .flatten();
    let Some((generation, job, half_made)) = begun else {
        return;
    };
    if let Some(job) = job {
        job.cancel();
    }
    if let Some(half) = half_made {
        discard(e, half);
    }

    let old = e.untrack(|| e.read(me, Loader::ITEM.index).as_object());
    if let Some(old) = old.filter(|o| e.is_alive(*o)) {
        // The item hears of its end while it is still the Loader's.
        if with_run(e, me, |r| r.leaving.replace(old) != Some(old)) == Some(true) {
            e.announce_destruction(old);
        }
    }
    let Some(origin) = want else {
        if named {
            // Switched off: Qt keeps `progress`.
            set(e, me, Loader::ITEM.index, Value::Null);
            set(e, me, Loader::STATUS.index, loader_status::NULL.into());
        } else {
            set(e, me, Loader::STATUS.index, loader_status::NULL.into());
            set(e, me, Loader::PROGRESS.index, 0.0.into());
            set(e, me, Loader::ITEM.index, Value::Null);
        }
        drop_old(e, me, old);
        return;
    };
    // An asynchronous Loader tells the empty item while the next one is
    // made; a synchronous one goes from the old item straight to the new.
    if asynchronous {
        set(e, me, Loader::ITEM.index, Value::Null);
    } else if old.is_some() {
        clear_item_quietly(e, me);
    }
    drop_old(e, me, old);
    if !is_current(e, me, generation) {
        return;
    }
    let props = with_run(e, me, |r| r.props.clone()).unwrap_or_default();
    if asynchronous {
        start_job(e, me, generation, &origin, props);
        return;
    }
    // Made at once, the new item takes the old one's place with no empty
    // item told between (qml6).
    let made = match &origin {
        Origin::Url(url) => {
            e.create_from_url_in(&e.resolve_url(me, url), Some(me), props, Some(me))
        }
        Origin::Component(c) => e.component_create(*c, Some(me), props),
    };
    finish(e, me, generation, made);
}

/// Empties `item` while the next one is made, without a change: in Qt
/// `itemChanged` goes from the old item straight to the new one, and what
/// reads `item` in between reads null (checked on qml6).
fn clear_item_quietly(e: &Engine, me: ObjectId) {
    let quiet = e
        .property_node(me, Loader::ITEM.index)
        .is_ok_and(|n| e.graph().replace_quietly(n, Value::Null).unwrap_or(false));
    if quiet {
        // The field too, so what reads it or tells it later agrees.
        let _ = e.with_mut(me, |l: &mut Loader| {
            l.item = None;
            l.run.emptied_quietly = true;
        });
    } else {
        set(e, me, Loader::ITEM.index, Value::Null);
    }
}

fn is_current(e: &Engine, me: ObjectId, generation: u64) -> bool {
    e.is_alive(me) && with_run(e, me, |r| r.generation) == Some(generation)
}

fn start_job(
    e: &Engine,
    me: ObjectId,
    generation: u64,
    origin: &Origin,
    props: Vec<(String, Value)>,
) {
    // The Loader is the boundary of what it makes, and so its owner.
    let owner = e.boundary_proc(me);
    let (job, handle) = match origin {
        Origin::Url(url) => {
            let url = e.resolve_url(me, url);
            let (job, handle) = CreationJob::new(e, &url, Some(me), props, Lane::Visible, owner);
            (job.in_scope(me), handle)
        }
        Origin::Component(c) => {
            CreationJob::for_component(e, *c, Some(me), props, Lane::Visible, owner)
        }
    };
    with_run(e, me, |r| r.job = Some(handle));
    set(e, me, Loader::STATUS.index, loader_status::LOADING.into());
    set(e, me, Loader::PROGRESS.index, 0.0.into());
    // A status handler may have changed the Loader's mind.
    if !is_current(e, me, generation) {
        return;
    }
    e.incubate(job.on_done(move |e, made| finish(e, me, generation, made.clone())));
}

/// Publishes what a creation made, or its error.
fn finish(e: &Engine, me: ObjectId, generation: u64, made: Result<ObjectId, EngineError>) {
    if !is_current(e, me, generation) {
        // Something in the new object reloaded or destroyed the Loader.
        if let Ok(obj) = made {
            discard(e, obj);
        }
        return;
    }
    let emptied_quietly = with_run(e, me, |r| {
        r.job = None;
        r.incubating = None;
        std::mem::take(&mut r.emptied_quietly)
    })
    .unwrap_or(false);
    match made {
        Ok(obj) => {
            if !is_item(e, obj) {
                // Qt keeps a loaded non-visual object out of the lists.
                let _ = tree::forget_resource(e, me, obj);
            }
            set(e, me, Loader::ITEM.index, Value::Object(obj));
            set(e, me, Loader::STATUS.index, loader_status::READY.into());
            set(e, me, Loader::PROGRESS.index, 1.0.into());
            if is_current(e, me, generation) {
                let _ = e.emit_by_name(me, "loaded", &[]);
            }
        }
        Err(err) => {
            warn(e, err.to_string());
            set(e, me, Loader::STATUS.index, loader_status::ERROR.into());
            set(e, me, Loader::ITEM.index, Value::Null);
            if emptied_quietly {
                let _ = e.notify(me, Loader::ITEM.index);
            }
            set(e, me, Loader::PROGRESS.index, 1.0.into());
        }
    }
}

fn drop_old(e: &Engine, me: ObjectId, old: Option<ObjectId>) {
    let Some(old) = old else { return };
    with_run(e, me, |r| {
        if r.leaving == Some(old) {
            r.leaving = None;
        }
    });
    discard(e, old);
}

/// Takes `obj` off its parent now and destroys it at the next frame: a
/// handler of its own may be what is running. It hears of its end first,
/// while it is still the Loader's (once: a switch has told it already).
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

/// Keeps the Loader's and the item's sizes together along one axis.
fn resize(e: &Engine, me: ObjectId, horizontal: bool) -> Result<(), EngineError> {
    let (size, implicit) = if horizontal {
        (Item::WIDTH.index, Item::IMPLICIT_WIDTH.index)
    } else {
        (Item::HEIGHT.index, Item::IMPLICIT_HEIGHT.index)
    };
    let own = e.read(me, size).to_number();
    let follows = size_follows_implicit(e, me, horizontal);
    let Some(item) = e.read(me, Loader::ITEM.index).as_object() else {
        // An unloaded Loader keeps the size it had, as in Qt.
        return Ok(());
    };
    if !e.is_alive(item) || !is_item(e, item) {
        return Ok(());
    }
    if follows {
        let v = e.read(item, size);
        return write_computed(e, me, implicit, v);
    }
    if e.untrack(|| e.read(item, size)).to_number() != own {
        e.write_internal(item, size, own.into(), WriteKind::Imperative)?;
    }
    let v = e.read(item, implicit);
    write_computed(e, me, implicit, v)
}
