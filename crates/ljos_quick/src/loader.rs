//! `Loader`: makes one item from a `Component` or a file, and drops or
//! replaces it when `active`, `source` or `sourceComponent` change.
//!
//! A plain Loader makes its item on the spot. An `asynchronous` one hands
//! a [`ljos_engine::CreationJob`] to the engine ([`Engine::incubate`]); the scene's
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
//! A JS run cut short, or a script heap that ran out, is not the item's
//! fault: what it did before may have finished the work, the data may have
//! changed, and the garbage a collection frees may make room. So after one
//! the Loader loads again by itself, and the log says so. It never gives
//! up for good, unlike Qt, which has neither limit (a script that never
//! returns hangs it, and running out of memory ends it). A fault soon after
//! the Loader loaded again ([`SECOND_CHANCE`]) makes it wait before the
//! next load, twice as long each time up to [`LONGEST_WAIT`] ([`Backoff`]),
//! so an item that runs out each time costs one load in that long, and one
//! that ran out while something else filled the heap comes back when that
//! has let go. Told to load (a change of `active`, `source` or
//! `sourceComponent`), it loads at once.
//! That count is for a run that loading again could cut short again: one
//! while the item is made, or in the item's own code. A short handler of a
//! made item whose dispatch was cut short in other objects' code (a click
//! that sets off long work elsewhere) was set off by whatever ran the
//! handler, not by the loading, so the Loader loads again each time at
//! once. When the interrupted dispatch was not input someone gave (a press,
//! a key, a wheel: [`crate::input::filed_acting`]) but a timer's or other
//! work's, the item set the work off by itself: [`ELSEWHERE_STREAK`] such
//! interrupts in a row, each of an item the last one loaded again and soon
//! after it, make it wait as above before each load after that.
//! Before loading again for a full heap the Loader collects the garbage;
//! faults are delivered between runs, where it can.
//! An exception in a handler is not such a fault: Qt logs it and keeps
//! the item, and so does this. The work of making the item is charged to
//! the Loader as its owner.

use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Atom, Boundary, BoundaryConfig, CreationHandle, Cx, Engine, EngineError, FaultReport, Initial,
    Interception, LogLevel, MetaRef, Migrate, ObjectId, PropIndex, QmlObject, QmlType, Request,
    Url, Value, WriteKind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, Lane, Restart, Time};

use crate::enums::loader_status;
use crate::item::{Item, is_a, is_item, size_follows_implicit, write_computed};
use crate::tree;

/// How long after loading again for an interrupted script or a full heap a
/// Loader takes another such fault as its item's doing, and waits before it
/// loads again ([`Backoff`]).
const SECOND_CHANCE: Duration = Duration::from_secs(10);

/// The wait before the first load that follows a fault soon after the last
/// load; each such fault after it doubles the wait.
const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between loads: an item that runs out of time or memory
/// each time it is made is made once in this long, and never given up on.
const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// How many interrupts in a row of scripts an item set off elsewhere by
/// itself (a timer of its own, not input someone gave it) a Loader takes,
/// each within [`SECOND_CHANCE`] of the one before and of the item it
/// loaded again, before it waits between loads as [`Backoff`] does. An item
/// that sets the work off again each time it is made would chain them for
/// good, however long the cut-off scripts ran.
const ELSEWHERE_STREAK: u32 = 8;

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
    /// How `props` are set: as `incubateObject` sets its own (qml6), told
    /// at the `setSource` call.
    props_how: Initial,
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
    /// When the Loader loads again after interrupted scripts or a full
    /// heap.
    backoff: Backoff,
    /// The load [`Backoff`] waits for, to be dropped when the Loader is
    /// told to load before it: its timer, when it is due, and how it is
    /// counted.
    waiting: Option<(ljos_runtime::TimerId, Time, Count)>,
    /// What stopped the script of the last fault [`Backoff`] counted.
    last_cut: Option<Cut>,
    /// When a script an item set off elsewhere by itself was last
    /// interrupted, how many were in a row, and the generation of the item
    /// that loaded again for it: an item any other load made starts anew.
    elsewhere: Option<(Time, u32, u64)>,
    /// [`Engine::faults_reported`] when the item was last made: a fault
    /// reported by then came from making it. `u64::MAX` while it is being
    /// published.
    made_through: u64,
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
                    r.props_how = Initial::Native;
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
        // What the item cannot take is the caller's error and the item is
        // made all the same (qml6), where a native caller's stops it.
        let how = Initial::incubated_here(e, &props);
        let url = source.0;
        with_run(e, me, |r| {
            r.props = props;
            r.props_for = url.clone();
            r.props_how = how;
        });
        let before = with_run(e, me, |r| r.generation);
        e.batch(|| {
            set(e, me, Loader::SOURCE_COMPONENT.index, Value::Null);
            set(e, me, Loader::SOURCE.index, Value::Url(url));
        });
        // A new source has loaded already, from its watch: loading it again
        // would make a second item, whose first stays alive until the next
        // frame and acts on its own. Qt reloads an unchanged source too.
        if with_run(e, me, |r| r.generation) == before {
            reload(e, me, true);
        }
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

/// Makes every Loader a boundary and a focus scope.
pub(crate) fn register(engine: &Engine) {
    engine.register_boundary::<Loader>(Rc::new(LoaderBoundary));
    // Only asked while a reload is tracked (`ljos run --watch`).
    engine.register_migrate::<Loader>();
    // As in Qt, so that each loaded item's `focus` stays inside its own
    // Loader: two loaded items that both ask for focus keep it, and one
    // has active focus only while its Loader has focus.
    crate::focus::register_focus_scope(engine, MetaRef::Static(Loader::static_meta()));
}

/// What a `setSource` call gave the item, carried across a dev reload:
/// the Loader made from the new file makes its item as the call did. The
/// source itself is a value the reload carries on its own.
#[derive(Debug)]
pub struct CarriedCall {
    props: Vec<(String, Value)>,
    props_for: Atom,
    how: Initial,
}

impl Migrate for Loader {
    const VERSION: u32 = 1;
    type State = CarriedCall;

    fn snapshot(engine: &Engine, obj: ObjectId) -> Option<CarriedCall> {
        // Not for an error boundary's restart in a shipped app: what the
        // call gave may be what failed.
        if !engine.reload_tracking() {
            return None;
        }
        with_run(engine, obj, |r| {
            (!r.props.is_empty()).then(|| CarriedCall {
                props: r.props.clone(),
                props_for: r.props_for.clone(),
                how: r.props_how.clone(),
            })
        })
        .flatten()
    }

    fn migrate(
        engine: &Engine,
        obj: ObjectId,
        _from: u32,
        state: CarriedCall,
    ) -> Result<(), String> {
        // Translated now: the old tree's objects have counterparts only
        // until the reload is finished.
        let props = state
            .props
            .into_iter()
            .filter_map(|(name, v)| Some((name, engine.reload_translate(v)?)))
            .collect();
        let loaded = with_run(engine, obj, |r| {
            r.props = props;
            r.props_for = state.props_for;
            r.props_how = state.how;
            r.completed && r.origin == Some(Origin::Url(r.props_for.clone()))
        })
        .ok_or("not a Loader")?;
        // A Loader that names the file itself has loaded it without them by
        // now: a migrator runs once the remade tree is complete, so that
        // item, its handlers run, goes and is made again with them. For
        // one that loads in the background, `item` is null in between.
        if loaded {
            reload(engine, obj, true);
        }
        Ok(())
    }
}

struct LoaderBoundary;

impl Boundary for LoaderBoundary {
    fn config(&self, _: &Engine, _: ObjectId) -> BoundaryConfig {
        BoundaryConfig {
            // The supervisor restarts nothing: only an interrupted script
            // or a full heap has the Loader load again (`stop`), and it
            // counts those itself.
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
            let made_in = r.generation;
            r.generation += 1;
            let making = r.job.is_some() || report.seq <= r.made_through;
            let again = if making || !report.stopped_outside(me) {
                Again::Counted
            } else if crate::input::filed_acting(e, report.seq) {
                Again::Acted
            } else {
                Again::Elsewhere { made_in }
            };
            (r.job.take(), r.incubating.take(), again)
        });
        let Some((job, half_made, again)) = begun else {
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
        match report.fault {
            Fault::JsInterrupted { .. } => load_again(e, me, report.at, again, Cut::Interrupted),
            // The heap may have room once the garbage is collected; faults
            // are delivered between runs, where it can be. Loading again
            // could run out again, so it is counted as an interrupt made
            // while the item was made.
            Fault::JsOutOfMemory => {
                // Rows run out together: the first Loader told collects,
                // and the rest, whose faults came before it, load into
                // what it freed.
                let collected = e.extension::<CollectedThrough>();
                if report.seq > collected.0.get() && e.collect_garbage() {
                    collected.0.set(e.faults_reported());
                }
                load_again(e, me, report.at, Again::Counted, Cut::OutOfMemory);
            }
            _ => {}
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

/// Whether an interrupt counts toward a Loader's giving up on its item.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Again {
    /// Loading again could be cut short again: the run was the item's own
    /// code, or ran while the item was made.
    Counted,
    /// The item's handler set off other objects' code after it was made,
    /// the interrupt stopped that, and the dispatch was input someone
    /// gave: they set it off, and may well again.
    Acted,
    /// As [`Again::Acted`], for a dispatch no input was: the item set it
    /// off by itself. `made_in` is the generation of the item.
    Elsewhere { made_in: u64 },
}

/// [`Engine::faults_reported`] when a Loader last collected the garbage
/// for a full heap: a fault reported by then needs no other collection.
#[derive(Default)]
struct CollectedThrough(std::cell::Cell<u64>);

/// What stopped a script a Loader loads again for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    /// The run was interrupted.
    Interrupted,
    /// The script heap had no room for what it allocated.
    OutOfMemory,
}

/// When a Loader loads again after faults that are not its item's code's
/// own (an interrupted script, a full heap): at once after the first, and
/// after a wait for each that comes within [`SECOND_CHANCE`] of the load
/// before it, from [`FIRST_WAIT`] doubling up to [`LONGEST_WAIT`]. There is
/// no last time: the fault may have been something else's (another item
/// filling the heap), and what caused it may go away.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Backoff {
    /// When the Loader last loaded again for such a fault.
    last_load: Option<Time>,
    /// How many such faults in a row came soon after the load before.
    streak: u32,
}

impl Backoff {
    /// A fault at `at` the Loader loads again for: how long it waits first.
    pub(crate) fn fault(&mut self, at: Time) -> Duration {
        let soon = self
            .last_load
            .is_some_and(|t| at.saturating_since(t) < SECOND_CHANCE);
        self.streak = if soon {
            self.streak.saturating_add(1)
        } else {
            0
        };
        wait_for(self.streak)
    }

    /// The Loader loads again at `at`.
    pub(crate) fn loading(&mut self, at: Time) {
        self.last_load = Some(at);
    }
}

/// The wait before a load that follows `streak` faults in a row, each soon
/// after the load before: none for the first.
pub(crate) fn wait_for(streak: u32) -> Duration {
    match streak {
        0 => Duration::ZERO,
        n => FIRST_WAIT
            .saturating_mul(1_u32.checked_shl(n - 1).unwrap_or(u32::MAX))
            .min(LONGEST_WAIT),
    }
}

/// What the Loader notes when it loads again for a fault.
#[derive(Clone, Copy, Debug)]
enum Count {
    /// [`Run::backoff`].
    Backoff,
    /// [`Run::elsewhere`]: the `n`th interrupt in a row of work the item
    /// set off by itself.
    Elsewhere(u32),
    /// Nothing: someone's input set the work off.
    Acted,
}

/// After a script was stopped at `at`: loads again, now or after a wait.
/// The log has the fault already; this says what becomes of the item.
fn load_again(e: &Engine, me: ObjectId, at: Time, again: Again, cut: Cut) {
    // A fault while it waits (in the Loader's own code: it holds no item)
    // is not another load's: the wait stands, for the generation the
    // fault moved on to.
    if let Some(Some((old, due, count))) = with_run(e, me, |r| r.waiting.take()) {
        e.cancel_call(old);
        wait_until(e, me, due, count);
        return;
    }
    let place = e.declared_at(me).unwrap_or_default();
    let what = |cut| match cut {
        Cut::Interrupted => "a script was interrupted",
        Cut::OutOfMemory => "its item ran out of script memory",
    };
    let (wait, count, message) = if again == Again::Counted {
        let Some((wait, was)) = with_run(e, me, |r| (r.backoff.fault(at), r.last_cut.replace(cut)))
        else {
            return;
        };
        let message = if wait.is_zero() {
            format!("{place}Loader: {}, loading again", what(cut))
        } else {
            // The fault before it was another kind: not "again".
            let after = match was {
                Some(was) if was != cut => format!(" soon after {}", what(was)),
                _ => " again".to_owned(),
            };
            format!(
                "{place}Loader: {}{after}, so it stays at Loader.Error and loads again in {}",
                what(cut),
                seconds(wait)
            )
        };
        (wait, Count::Backoff, message)
    } else {
        let streak = with_run(e, me, |r| match (again, r.elsewhere) {
            (Again::Elsewhere { made_in }, Some((t, n, g)))
                if g == made_in && at.saturating_since(t) < SECOND_CHANCE =>
            {
                n + 1
            }
            (Again::Elsewhere { .. }, _) => 1,
            _ => 0,
        });
        let Some(n) = streak else { return };
        let count = if n == 0 {
            Count::Acted
        } else {
            Count::Elsewhere(n)
        };
        if n < ELSEWHERE_STREAK {
            let message = format!(
                "{place}Loader: a script its item set off in another object was interrupted, loading again"
            );
            (Duration::ZERO, count, message)
        } else {
            let wait = wait_for(n - ELSEWHERE_STREAK + 1);
            let message = format!(
                "{place}Loader: scripts its item set off in other objects were interrupted {n} times in a row, so it stays at Loader.Error and loads again in {}",
                seconds(wait)
            );
            (wait, count, message)
        }
    };
    warn(e, message);
    if wait.is_zero() {
        load_counted(e, me, at, count);
        return;
    }
    wait_until(e, me, at + wait, count);
}

/// Loads again at `due`, unless told to load before.
fn wait_until(e: &Engine, me: ObjectId, due: Time, count: Count) {
    let Some(generation) = with_run(e, me, |r| r.generation) else {
        return;
    };
    let timer = e.call_at(due, move |e| {
        // Told to load, or gone, since: that load stands.
        if !is_current(e, me, generation) {
            return;
        }
        with_run(e, me, |r| r.waiting = None);
        load_counted(e, me, e.now(), count);
    });
    if let Some(Some((old, ..))) = with_run(e, me, |r| r.waiting.replace((timer, due, count))) {
        e.cancel_call(old);
    }
}

/// Loads again for a fault, noting it as `count` says.
fn load_counted(e: &Engine, me: ObjectId, at: Time, count: Count) {
    let before = with_run(e, me, |r| {
        if let Count::Backoff = count {
            r.backoff.loading(at);
        }
        r.generation
    });
    reload(e, me, true);
    if let Count::Elsewhere(n) = count {
        // A load asked for while the item was made (by a `loaded` handler)
        // starts anew.
        with_run(e, me, |r| {
            let mine = before.map(|g| g + 1);
            r.elsewhere = (mine == Some(r.generation)).then_some((at, n, r.generation));
        });
    } else if let Count::Acted = count {
        with_run(e, me, |r| r.elsewhere = None);
    }
}

/// `1 s`, `1.5 s`.
fn seconds(d: Duration) -> String {
    format!("{} s", d.as_secs_f64())
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
        Some((
            r.generation,
            r.job.take(),
            r.incubating.take(),
            r.waiting.take(),
        ))
    })
    .flatten();
    let Some((generation, job, half_made, waiting)) = begun else {
        return;
    };
    // This load is the one a fault's wait was for.
    if let Some((timer, ..)) = waiting {
        e.cancel_call(timer);
    }
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
    // setSource's properties are for the source: a sourceComponent set
    // after `setSource("", props)` is made without them (qml6).
    let (props, how) = match origin {
        Origin::Url(_) => {
            with_run(e, me, |r| (r.props.clone(), r.props_how.clone())).unwrap_or_default()
        }
        Origin::Component(_) => Default::default(),
    };
    let req = match &origin {
        Origin::Url(url) => Request::url(e.resolve_url(me, url)).in_scope(me),
        Origin::Component(c) => Request::component(*c),
    }
    .parent(Some(me))
    .props(props, how);
    if asynchronous {
        start_job(e, me, generation, req);
        return;
    }
    // Made at once, the new item takes the old one's place with no empty
    // item told between (qml6).
    finish(e, me, generation, e.make(req).map(|(item, _)| item));
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

fn start_job(e: &Engine, me: ObjectId, generation: u64, req: Request) {
    // The Loader is the boundary of what it makes, and so its owner.
    let owner = e.boundary_proc(me);
    let (job, handle) = e.make_later(req, Lane::Visible, owner);
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
        // What `loaded` or a change of `item` sets off is still making it.
        r.made_through = u64::MAX;
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
            // A script cut short while the item is made is no error here:
            // making it goes on, and its fault reaches `stop`.
            warn(e, err.to_string());
            set(e, me, Loader::STATUS.index, loader_status::ERROR.into());
            set(e, me, Loader::ITEM.index, Value::Null);
            if emptied_quietly {
                let _ = e.notify(me, Loader::ITEM.index);
            }
            set(e, me, Loader::PROGRESS.index, 1.0.into());
        }
    }
    if is_current(e, me, generation) {
        with_run(e, me, |r| r.made_through = e.faults_reported());
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// What happens to a Loader whose item faults over and over: when, after
    /// each load, the next fault comes, or `None` when the item stays up.
    #[derive(Clone, Debug)]
    enum Step {
        /// The item faults this long after it was loaded.
        Faults(Duration),
        /// The item stays up this long, then faults.
        Lasts(Duration),
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            (0..SECOND_CHANCE.as_millis() as u64)
                .prop_map(|ms| Step::Faults(Duration::from_millis(ms))),
            (SECOND_CHANCE.as_millis() as u64..10 * SECOND_CHANCE.as_millis() as u64)
                .prop_map(|ms| Step::Lasts(Duration::from_millis(ms))),
        ]
    }

    proptest! {
        /// Run against a model of the state machine: every fault is followed
        /// by a load (never a last time), a fault after an item that lasted
        /// is loaded again at once, and a streak of quick faults waits
        /// twice as long each time, from a second up to a minute.
        #[test]
        fn a_loader_always_loads_again_and_waits_as_the_model_says(
            steps in proptest::collection::vec(step(), 1..40),
        ) {
            let mut b = Backoff::default();
            let mut now = Time::ZERO + Duration::from_secs(1);
            // The first load is the application's own, not counted.
            let mut model_streak: Option<u32> = None;
            for s in steps {
                let (quick, after) = match s {
                    Step::Faults(d) => (true, d),
                    Step::Lasts(d) => (false, d),
                };
                now += after;
                let wait = b.fault(now);
                let streak = match (model_streak, quick) {
                    (Some(n), true) => n + 1,
                    _ => 0,
                };
                let expected = if streak == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1u64 << (streak - 1).min(6)).min(LONGEST_WAIT)
                };
                prop_assert_eq!(wait, expected);
                prop_assert!(wait <= LONGEST_WAIT);
                now += wait;
                b.loading(now);
                model_streak = Some(streak);
            }
        }

        /// An item that faults at once every time is loaded at most once a
        /// minute once the waits are long, and still loaded.
        #[test]
        fn a_runaway_item_is_loaded_at_a_bounded_rate(n in 8u32..200) {
            let mut b = Backoff::default();
            let mut now = Time::ZERO;
            let mut loads = Vec::new();
            for _ in 0..n {
                now += b.fault(now);
                b.loading(now);
                loads.push(now);
            }
            prop_assert_eq!(loads.len() as u32, n);
            for w in loads.windows(2).skip(7) {
                prop_assert_eq!(w[1].saturating_since(w[0]), LONGEST_WAIT);
            }
        }
    }

    #[test]
    fn waits_double_from_a_second_to_a_minute() {
        let waits: Vec<u64> = (0..10).map(|n| wait_for(n).as_secs()).collect();
        assert_eq!(waits, [0, 1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(wait_for(u32::MAX), LONGEST_WAIT);
    }
}
