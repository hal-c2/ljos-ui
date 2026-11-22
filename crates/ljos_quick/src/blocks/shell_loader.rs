//! `ShellLoader`: shows a shell a user may have written, or the
//! application's own when the user's does not work.
//!
//! The shell at `source` is made at once. When it does not compile, or a
//! fault is raised in it while it is made (an exception in a handler or a
//! binding, a panic in a native type), it is discarded and the shell at
//! `fallback` is made in its place; `error` says why, with the file and
//! line. A fault in the user's shell later on does the same: the loader
//! is the boundary of its source's shell. Changing `source` or `fallback`
//! tries again, and an `error` goes once the source's shell is shown.
//!
//! A try from a shown fallback, after `reload()` or a resource fault,
//! holds that fallback until the shell's first read is over
//! ([`first_read`]): made is not working, as bindings are lazy. Meanwhile
//! `item` is the shell, `status` is Ready and `error` still says why the
//! fallback was shown. A fault in that time shows the same object again;
//! otherwise it is destroyed and `error` goes. A changed `source` or
//! `fallback` holds nothing. Both shells are in the heap through that
//! read, so a source that fits only without the fallback is not shown.
//!
//! A resource fault (the script heap ran out, the shell could not be made
//! in its time) is not the shell's own: the fallback stands in for it
//! while the loader waits as a Loader does ([`Backoff`]: a second, twice
//! as long each time the shell fails again soon after, up to a minute),
//! and it then tries the source again, for as long as it takes. A script
//! of a shell that was made, stopped for its time, leaves the shell
//! shown: the engine runs the binding it cut short again, and a shell
//! made again would blank, and run over again, without end.
//!
//! It is not a `Loader`: a Loader only logs why it could not load, and it
//! holds one URL. The two make their item the same way.
//!
//! A shell may be a `Window`. Where each `Window` is a platform window of
//! its own, a loader that is the root of what a window shows stands for
//! its shell there ([`RootSwaps::present`]): the platform window shows the
//! shell's `Window`, not the loader and another window beside it, and a
//! shell that takes its place, a `Window` or an item, takes that platform
//! window at the size it has.
//!
//! Under `--watch` a saved file is reloaded by making the application's
//! roots again (ARCHITECTURE §12.1), and so the loader tries its source
//! again. A shell that was shown and that an edit breaks is not replaced
//! by the fallback: a reload that does not compile keeps the generation
//! that runs, and says why in the reload error, not in `error`.
//!
//! `reload()` asks for the same where nothing watches the files, or the
//! watch missed a save: a host that reads them ([`Rereads`]) reads every
//! one again and applies what changed as it applies a save. Unless a file
//! failed, or where no host reads the files, the loader tries its source
//! again then: a shell that is shown is kept, and one that did not work
//! replaces the fallback once it does.

use std::time::Duration;

use ljos_engine::backoff::{Backoff, seconds};
use ljos_engine::{
    Atom, Boundary, BoundaryConfig, Cx, Engine, EngineError, FaultReport, LogLevel, ObjectId,
    PropIndex, QmlObject, Request, Url, Value, WriteKind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, Restart, TimerId};

use super::contribution::text;
use crate::first_read;
use crate::focus::FocusReason;
use crate::focus::tree::{force_active_focus, is_under, top_of, tree_focus_item};
use crate::item::{Item, is_item, size_follows_implicit, write_computed};
use crate::reread::Rereads;
use crate::roots::{RootSwaps, lasts};
use crate::tree;
use crate::views::cull;

/// `ShellLoader.status`.
pub mod shell_status {
    /// Neither a source nor a fallback.
    pub const NULL: i32 = 0;
    /// The source's shell is shown.
    pub const READY: i32 = 1;
    /// The fallback is shown; `error` says why.
    pub const FALLBACK: i32 = 2;
    /// Nothing could be shown; `error` says why for both.
    pub const ERROR: i32 = 3;
}

fn shell_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    use shell_status as s;
    vec![(
        "Status",
        crate::enums::keys(&[
            ("Null", s::NULL),
            ("Ready", s::READY),
            ("Fallback", s::FALLBACK),
            ("Error", s::ERROR),
        ]),
    )]
}

/// `ShellLoader` (Ljos.Blocks).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "Ljos.Blocks",
    name = "ShellLoader",
    extends = Item,
    enums = shell_enums
)]
pub struct ShellLoader {
    #[qml(base)]
    pub item_base: Item,
    /// The shell to show.
    #[qml(notify)]
    pub source: Url,
    /// The shell shown when the source's does not work.
    #[qml(notify)]
    pub fallback: Url,
    /// Why the source's shell is not shown: `file:line: message`. Empty
    /// while it is, but through the first read of one tried again from
    /// the fallback.
    #[qml(notify, readonly)]
    pub error: Atom,
    /// The shell shown.
    #[qml(notify, readonly)]
    pub item: Option<ObjectId>,
    /// [`shell_status`]. Ready, with the `error` there was, through the
    /// first read of a shell tried again from the fallback.
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(skip)]
    run: Run,
}

#[derive(Debug, Default)]
struct Run {
    completed: bool,
    /// The fallback is being made or is shown: a fault in it is not the
    /// loader's to handle.
    making_fallback: bool,
    /// A shell is being made, or the one shown is being dropped for it:
    /// one asked to be tried again meanwhile reads the source as it is
    /// already.
    making: bool,
    /// How often another shell was asked for, by [`load`] or [`retry`]:
    /// what ran code of the user's sees by it that the code asked, and
    /// what that showed is then left as it is.
    loads: u64,
    fallback_item: Option<ObjectId>,
    /// The fallback while it is held through the first read of the shell
    /// that replaces it.
    held: Option<Held>,
    /// Faults up to this one were dealt with while a shell was made; the
    /// boundary hears of them only afterwards.
    handled: u64,
    /// When the source is tried again after a resource fault.
    backoff: Backoff,
    /// The try [`Run::backoff`] waits for.
    waiting: Option<TimerId>,
}

/// A fallback held through a first read ([`first_read`]).
#[derive(Clone, Copy, Debug)]
struct Held {
    fallback: ObjectId,
    /// The item of it that had the keyboard.
    focus: Option<ObjectId>,
    /// The hold's number.
    n: u64,
}

#[qml_impl]
impl ShellLoader {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        for p in [ShellLoader::SOURCE.index, ShellLoader::FALLBACK.index] {
            e.watch(me, p, move |e, _| load(e, me))?;
        }
        e.effect_in(me, move |e| {
            for horizontal in [true, false] {
                if let Err(err) = resize(e, me, horizontal) {
                    warn(e, format!("ShellLoader: cannot size the shell: {err}"));
                }
            }
        })?;
        with_run(e, me, |r| r.completed = true);
        load(e, me);
        Ok(())
    }

    /// Reads the files again and tries the source again ([`Rereads`]).
    #[qml(invokable, name = "reload")]
    fn reload(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        // A host that reads the files applies what changed by making the
        // roots again, this loader with them; it answers with `retry`
        // unless a file failed.
        if !Rereads::of(e).ask_for(me) {
            retry(e, me);
        }
    }
}

/// Tries the source again, as asked. A source's shell that is shown is
/// kept, with what the user did in it: its files are as they were, or
/// what changed would have made it again. One that did not work replaces
/// the fallback once it was read without a fault, and says why again when
/// it still does not work.
pub(crate) fn retry(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) || with_run(e, me, |r| r.completed) != Some(true) {
        return;
    }
    // Asked by the shell being made, as it completes: making it again
    // would make a second shell of the same files under the first.
    if with_run(e, me, |r| r.making) == Some(true) {
        return;
    }
    let status = e.untrack(|| e.read(me, ShellLoader::STATUS.index).to_number()) as i32;
    if status == shell_status::READY {
        return;
    }
    let source = url(e, me, ShellLoader::SOURCE.index);
    if source.is_empty() && status == shell_status::FALLBACK {
        // Nothing to try: the fallback is what this loader shows.
        return;
    }
    if status == shell_status::NULL || source.is_empty() {
        load(e, me);
        return;
    }
    let Some(loads) = asked(e, me) else {
        return;
    };
    // The fallback stays shown until the source's shell is made: what
    // the user sees does not go blank while it is tried.
    let Some(made) = make(e, me, &source) else {
        return;
    };
    match made {
        Ok(obj) => {
            // A try still waiting out a resource fault would make the
            // shell again, and lose what the user does in this one.
            if let Some(Some(timer)) = with_run(e, me, |r| r.waiting.take()) {
                e.cancel_call(timer);
            }
            // Made is not working: bindings are lazy. A fallback that is
            // shown is held until the shell was read once, so that it is
            // the same one, with what the user did in it, if that fails.
            // One change to who hears of it: a handler of `itemChanged`
            // that asks for another shell finds this one shown, and Ready.
            e.batch(|| {
                if let Some(fallback) = shown_fallback(e, me, status) {
                    hold_for(e, me, fallback, obj);
                    set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
                    // What it let go of is held: none of it is destroyed
                    // here.
                    RootSwaps::of(e).present(e, me, None);
                    present(e, me, obj);
                    set(e, me, ShellLoader::STATUS.index, shell_status::READY.into());
                } else if drop_item(e, me) {
                    show_source(e, me, obj);
                } else {
                    discard(e, obj);
                }
            });
        }
        Err(Failed { why, resource }) => {
            warn(e, format!("ShellLoader: {why}"));
            if !still(e, me, loads) {
                return;
            }
            // None is shown where it was asked as that one was dropped.
            let shown = e
                .untrack(|| e.read(me, ShellLoader::ITEM.index))
                .as_object()
                .is_some();
            let again = if status == shell_status::FALLBACK && shown {
                set(e, me, ShellLoader::ERROR.index, why.as_str().into());
                Some(false)
            } else {
                show_fallback(e, me, loads, why)
            };
            if again.is_some_and(|again| again || resource) {
                again_after(e, me, loads, None);
            }
        }
    }
}

pub(super) struct Behaviour;

impl Boundary for Behaviour {
    fn config(&self, _: &Engine, _: ObjectId) -> BoundaryConfig {
        BoundaryConfig {
            // The fallback stands in until the source changes.
            restart: Restart::Temporary,
            intensity: Intensity::new(0, Duration::ZERO),
            // As for a Loader: after the shell is made, an exception is
            // logged and the shell stays.
            catch_exceptions: false,
        }
    }

    fn guards(&self, e: &Engine, me: ObjectId, child: ObjectId) -> bool {
        let guarded = e
            .with(me, |l: &ShellLoader| {
                !l.run.making_fallback && l.run.fallback_item != Some(child)
            })
            .unwrap_or(false);
        // A fallback's window that waits to give way is a fallback still.
        guarded && !RootSwaps::of(e).lets_go(me, child)
    }

    fn stop(&self, e: &Engine, me: ObjectId, report: &FaultReport) {
        let handled = with_run(e, me, |r| r.handled).unwrap_or(u64::MAX);
        if report.seq <= handled || failed(e, me) {
            return;
        }
        let Some(loads) = with_run(e, me, |r| r.loads) else {
            return;
        };
        let why = describe(report);
        if show_held_again(e, me, loads, &why) {
            if is_resource(&report.fault) {
                again_after(e, me, loads, Some(report));
            }
            return;
        }
        let goes_on = drop_item(e, me);
        warn(e, format!("ShellLoader: {why}"));
        if !goes_on {
            return;
        }
        if show_fallback(e, me, loads, why).is_some_and(|again| again || is_resource(&report.fault))
        {
            again_after(e, me, loads, Some(report));
        }
    }

    fn start(&self, _: &Engine, _: ObjectId) -> Result<(), Fault> {
        // Never asked of a temporary child.
        Ok(())
    }

    fn failed(&self, e: &Engine, me: ObjectId) -> bool {
        failed(e, me)
    }

    fn keeps(&self, _: &Engine, _: ObjectId, report: &FaultReport) -> bool {
        // A script of the shell stopped for its time leaves the shell
        // whole: the binding cut short runs again by itself
        // (`run_again_after`). A full heap or a panic is the supervisor's,
        // and so `stop`'s.
        kept(report)
    }
}

/// Whether `report` leaves the shell it was raised in shown: a script of
/// it stopped for its time. A shell that could not be made at all comes
/// back from [`make`] as an error, not as a fault in it.
fn kept(report: &FaultReport) -> bool {
    matches!(report.fault, Fault::JsInterrupted { .. })
}

fn with_run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut(me, |l: &mut ShellLoader| f(&mut l.run)).ok()
}

fn warn(e: &Engine, message: String) {
    e.log(LogLevel::Warning, "qml", message);
}

fn failed(e: &Engine, me: ObjectId) -> bool {
    let status = e.untrack(|| e.read(me, ShellLoader::STATUS.index).to_number()) as i32;
    status == shell_status::FALLBACK || status == shell_status::ERROR
}

fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        warn(e, format!("ShellLoader: {err}"));
    }
}

fn url(e: &Engine, me: ObjectId, p: PropIndex) -> String {
    match e.untrack(|| e.read(me, p)) {
        Value::Url(u) => u.as_str().to_owned(),
        other => text(&other),
    }
}

/// Another shell is asked for: returns the count that says so for as
/// long as no other is ([`still`]).
fn asked(e: &Engine, me: ObjectId) -> Option<u64> {
    with_run(e, me, |r| {
        r.loads += 1;
        // Asked as a fallback is made, what is asked for is guarded: that
        // fallback is not shown.
        r.making_fallback = false;
        r.loads
    })
}

/// Whether the loader is there and no other shell was asked for since
/// `loads`. Asked after whatever can run code of the user's (a handler,
/// a destruction, a collection, a listener of the log) and before what
/// is written next.
fn still(e: &Engine, me: ObjectId, loads: u64) -> bool {
    with_run(e, me, |r| r.loads) == Some(loads)
}

/// Says `error`, then `status`, each unless another shell was asked for
/// by then. False where one was, also by a handler of the last.
fn say_failed(e: &Engine, me: ObjectId, loads: u64, error: &str, status: i32) -> bool {
    if !still(e, me, loads) {
        return false;
    }
    set(e, me, ShellLoader::ERROR.index, error.into());
    if !still(e, me, loads) {
        return false;
    }
    set(e, me, ShellLoader::STATUS.index, status.into());
    still(e, me, loads)
}

/// Shows the source's shell, or the fallback's.
fn load(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) || with_run(e, me, |r| r.completed) != Some(true) {
        return;
    }
    let Some(loads) = asked(e, me) else {
        return;
    };
    if let Some(Some(timer)) = with_run(e, me, |r| r.waiting.take()) {
        e.cancel_call(timer);
    }
    // Asked for another shell, the loader keeps none for the last one.
    if let Some(held) = end_hold(e, me) {
        let_go_of(e, me, held.fallback);
    }
    if !still(e, me, loads) || !drop_item(e, me) {
        return;
    }
    let source = url(e, me, ShellLoader::SOURCE.index);
    if !still(e, me, loads) {
        return;
    }
    if source.is_empty() {
        if show_fallback(e, me, loads, String::new()) == Some(true) {
            again_after(e, me, loads, None);
        }
        return;
    }
    let Some(made) = make(e, me, &source) else {
        return;
    };
    match made {
        Ok(obj) => e.batch(|| show_source(e, me, obj)),
        Err(Failed { why, resource }) => {
            warn(e, format!("ShellLoader: {why}"));
            if show_fallback(e, me, loads, why).is_some_and(|again| again || resource) {
                again_after(e, me, loads, None);
            }
        }
    }
}

/// Whether `fault` is one the shell's code did not cause and a later try
/// may not meet. A panic in a native type is that type's bug, as for a
/// Loader.
fn is_resource(fault: &Fault) -> bool {
    matches!(fault, Fault::JsOutOfMemory | Fault::JsInterrupted { .. })
}

/// After a resource fault: tries the source again once the [`Backoff`]
/// wait is over, the fallback shown meanwhile. `report` is the fault told
/// to the shell, when one was; a shell that failed while it was made was
/// told of none. Nothing waits where another shell was asked for since
/// `loads`: that waits by itself.
fn again_after(e: &Engine, me: ObjectId, loads: u64, report: Option<&FaultReport>) {
    if !still(e, me, loads) {
        return;
    }
    let now = e.now();
    // A full heap may have room once the garbage is collected.
    match report {
        Some(report) => e.collect_garbage_for_fault(report),
        None => e.collect_garbage_for_full_heap(),
    };
    if !still(e, me, loads) {
        return;
    }
    let Some(wait) = with_run(e, me, |r| r.backoff.fault(now)) else {
        return;
    };
    warn(
        e,
        format!("ShellLoader: trying the source again in {}", seconds(wait)),
    );
    if !still(e, me, loads) {
        return;
    }
    let timer = e.call_at(now + wait, move |e| {
        let tried = with_run(e, me, |r| {
            r.waiting = None;
            r.backoff.trying(e.now());
        });
        if tried.is_some() {
            // The fallback stays shown while the source is tried.
            retry(e, me);
        }
    });
    if let Some(Some(old)) = with_run(e, me, |r| r.waiting.replace(timer)) {
        e.cancel_call(old);
    }
}

/// Why a shell could not be made, and whether it was a resource fault.
struct Failed {
    why: String,
    resource: bool,
}

/// Shows the fallback's shell in place of the source's, which did not
/// work for `why` (empty when there is no source). Returns whether the
/// fallback could not be made for want of room, so the caller tries the
/// source again: it calls [`again_after`] once per failed try, as each
/// call raises the backoff. `None` where another shell was asked for
/// since `loads`, as the fallback was made or by a handler of what is set
/// here: that showed what there is, and waits by itself.
fn show_fallback(e: &Engine, me: ObjectId, loads: u64, why: String) -> Option<bool> {
    let fallback = url(e, me, ShellLoader::FALLBACK.index);
    if !still(e, me, loads) {
        return None;
    }
    if fallback.is_empty() {
        let status = if why.is_empty() {
            shell_status::NULL
        } else {
            shell_status::ERROR
        };
        return say_failed(e, me, loads, &why, status).then_some(false);
    }
    with_run(e, me, |r| r.making_fallback = true);
    let made = make(e, me, &fallback)?;
    let mut again = false;
    match made {
        Ok(obj) => e.batch(|| {
            with_run(e, me, |r| {
                r.fallback_item = Some(obj);
                // Before a handler can ask for the source's shell, which
                // is guarded.
                r.making_fallback = false;
            });
            set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
            present(e, me, obj);
            set(e, me, ShellLoader::ERROR.index, why.as_str().into());
            set(
                e,
                me,
                ShellLoader::STATUS.index,
                shell_status::FALLBACK.into(),
            );
        }),
        Err(Failed {
            why: also,
            resource,
        }) => {
            again = resource;
            warn(e, format!("ShellLoader: fallback: {also}"));
            let both = if why.is_empty() {
                format!("fallback: {also}")
            } else {
                format!("{why}\nfallback: {also}")
            };
            if !say_failed(e, me, loads, &both, shell_status::ERROR) {
                return None;
            }
        }
    }
    if !still(e, me, loads) {
        return None;
    }
    with_run(e, me, |r| r.making_fallback = false);
    // Nothing could be shown for want of room: the source is tried again,
    // and the fallback with it.
    Some(again)
}

/// Shows `obj`, the source's shell, with nothing held.
fn show_source(e: &Engine, me: ObjectId, obj: ObjectId) {
    set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
    present(e, me, obj);
    set(e, me, ShellLoader::STATUS.index, shell_status::READY.into());
    set(e, me, ShellLoader::ERROR.index, "".into());
}

/// Makes the shell at `url`, or says why it could not: it did not
/// compile, or something in it faulted while it was made.
///
/// `None`: another was asked for meanwhile, as by the shell's own
/// `Component.onCompleted` setting `source`, or the loader went. What was
/// made is destroyed, and the caller neither shows nor says anything: a
/// [`load`] inside this did.
fn make(e: &Engine, me: ObjectId, url: &str) -> Option<Result<ObjectId, Failed>> {
    let (was, loads) = with_run(e, me, |r| (std::mem::replace(&mut r.making, true), r.loads))?;
    let (made, filed) = e.noting_faults(|| {
        e.make(
            Request::url(e.resolve_url(me, url))
                .parent(Some(me))
                .in_scope(me),
        )
        .map(|(shell, _)| shell)
    });
    with_run(e, me, |r| r.making = was);
    if with_run(e, me, |r| r.loads) != Some(loads) {
        // `handled` stays the last make's: a fault filed since may be in
        // the shell that is shown, and one in this finds it going.
        if let Ok(obj) = made {
            discard(e, obj);
        }
        return None;
    }
    let after = last_fault(e);
    with_run(e, me, |r| r.handled = after);
    let obj = match made {
        Ok(obj) => obj,
        Err(err) => {
            return Some(Err(Failed {
                resource: matches!(
                    err,
                    EngineError::JsOutOfMemory(_) | EngineError::JsInterrupted(_)
                ),
                why: err.to_string(),
            }));
        }
    };
    if let Some(report) = fault_in(e, obj, &filed) {
        let failed = Failed {
            why: describe(report),
            resource: is_resource(&report.fault),
        };
        discard(e, obj);
        // A handler of its going may have asked for another.
        if with_run(e, me, |r| r.loads) != Some(loads) {
            return None;
        }
        return Some(Err(failed));
    }
    // A binding that throws while the shell is made is a fault only if
    // it still throws once the creation around the loader is complete,
    // as an application starts: the shell is judged again then, by those
    // faults alone. One raised after it was made, as by a handler the
    // rest of the tree sets off as it completes, is the boundary's.
    e.after_creation_told(move |e, told| judge_again(e, me, obj, told));
    Some(Ok(obj))
}

/// The first of `faults` raised in `obj` or what it owns that breaks it
/// (not one that leaves it shown: [`kept`]).
fn fault_in<'a>(e: &Engine, obj: ObjectId, faults: &'a [FaultReport]) -> Option<&'a FaultReport> {
    faults
        .iter()
        .find(|r| !kept(r) && r.origin.is_some_and(|o| within(e, o, obj)))
}

/// What [`make`] would have said had the shell's faults in `told` been
/// told while it made it: the source's shell gives way to the fallback,
/// and a fallback leaves nothing shown.
fn judge_again(e: &Engine, me: ObjectId, obj: ObjectId, told: &[FaultReport]) {
    if !e.is_alive(me) {
        return;
    }
    let shown = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    if shown != Some(obj) {
        return;
    }
    let Some(report) = fault_in(e, obj, told) else {
        return;
    };
    let Some(loads) = with_run(e, me, |r| r.loads) else {
        return;
    };
    let why = describe(report);
    if with_run(e, me, |r| r.fallback_item) == Some(Some(obj)) {
        warn(e, format!("ShellLoader: fallback: {why}"));
        // Or the one a listener asked for is what is dropped.
        if !still(e, me, loads) {
            return;
        }
        let first = text(&e.untrack(|| e.read(me, ShellLoader::ERROR.index)));
        let both = if first.is_empty() {
            format!("fallback: {why}")
        } else {
            format!("{first}\nfallback: {why}")
        };
        if !drop_item(e, me) || !say_failed(e, me, loads, &both, shell_status::ERROR) {
            return;
        }
        if is_resource(&report.fault) {
            again_after(e, me, loads, Some(report));
        }
        return;
    }
    let resource = is_resource(&report.fault);
    if show_held_again(e, me, loads, &why) {
        if resource {
            again_after(e, me, loads, Some(report));
        }
        return;
    }
    let goes_on = drop_item(e, me);
    warn(e, format!("ShellLoader: {why}"));
    if !goes_on {
        return;
    }
    if show_fallback(e, me, loads, why).is_some_and(|again| again || resource) {
        again_after(e, me, loads, Some(report));
    }
}

fn last_fault(e: &Engine) -> u64 {
    e.faults().last().map_or(0, |r| r.seq)
}

/// Whether `obj` is `root` or owned by something `root` owns.
fn within(e: &Engine, obj: ObjectId, root: ObjectId) -> bool {
    let mut cur = Some(obj);
    // Ownership has no cycles; the bound is for a bug that makes one.
    for _ in 0..4096 {
        match cur {
            Some(o) if o == root => return true,
            Some(o) => cur = e.owner(o),
            None => return false,
        }
    }
    false
}

fn describe(report: &FaultReport) -> String {
    match &report.file {
        Some(file) if report.line > 0 => format!("{file}:{}: {}", report.line, report.message()),
        Some(file) => format!("{file}: {}", report.message()),
        None => report.message(),
    }
}

/// The fallback shown, where `status` says one is and it is to last.
fn shown_fallback(e: &Engine, me: ObjectId, status: i32) -> Option<ObjectId> {
    if status != shell_status::FALLBACK {
        return None;
    }
    let shown = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    let fallback = with_run(e, me, |r| r.fallback_item)?;
    fallback.filter(|f| shown == Some(*f) && lasts(e, *f))
}

/// Holds `fallback` until `shell`'s first read is over. An item is not
/// drawn, pointed at or described meanwhile; nothing of it is written.
fn hold_for(e: &Engine, me: ObjectId, fallback: ObjectId, shell: ObjectId) {
    let focus = tree_focus_item(e, e.untrack(|| top_of(e, fallback)))
        .filter(|it| is_under(e, *it, fallback));
    let window = own_window(e, me, fallback);
    if window {
        RootSwaps::of(e).hold(fallback);
    } else {
        cull::pool(e, fallback, true);
    }
    let out = (!window).then_some(fallback);
    let n = first_read::hold(e, me, shell, out, read_over);
    if !window {
        crate::scene::described(e, fallback);
    }
    with_run(e, me, |r| r.held = Some(Held { fallback, focus, n }));
}

/// Ends the hold there is: the fallback is as any other item again.
fn end_hold(e: &Engine, me: ObjectId) -> Option<Held> {
    let held = with_run(e, me, |r| r.held.take())??;
    first_read::end(e, held.n);
    cull::pool(e, held.fallback, false);
    crate::scene::described(e, held.fallback);
    Some(held)
}

/// The shell's first read is over and it did not fail: it has replaced
/// the fallback, and `error` goes.
fn read_over(e: &Engine, me: ObjectId, n: u64) {
    if with_run(e, me, |r| r.held.map(|h| h.n)) != Some(Some(n)) {
        return;
    }
    let Some(held) = end_hold(e, me) else {
        return;
    };
    let Some(loads) = with_run(e, me, |r| {
        r.fallback_item = None;
        r.loads
    }) else {
        return;
    };
    let_go_of(e, me, held.fallback);
    if still(e, me, loads) {
        set(e, me, ShellLoader::ERROR.index, "".into());
    }
}

/// Destroys `fallback`, which is held no longer, unless a window shows it
/// still: that one goes once the window shows what replaces it.
fn let_go_of(e: &Engine, me: ObjectId, fallback: ObjectId) {
    let swaps = RootSwaps::of(e);
    if swaps.lets_go(me, fallback) {
        swaps.release(fallback);
    } else {
        discard(e, fallback);
    }
}

/// The shell failed its first read for `why`: shows the fallback that was
/// held, the same object, in its place. False when none is held to show.
fn show_held_again(e: &Engine, me: ObjectId, loads: u64, why: &str) -> bool {
    let Some(Some(held)) = with_run(e, me, |r| r.held) else {
        return false;
    };
    if !lasts(e, held.fallback) {
        end_hold(e, me);
        return false;
    }
    let fallback = held.fallback;
    let shell = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    // Judged before anything moves: what took the keyboard outside the
    // loader since keeps it.
    let taken = tree_focus_item(e, e.untrack(|| top_of(e, fallback))).is_some_and(|it| {
        !is_under(e, it, fallback)
            && !shell.is_some_and(|s| is_under(e, it, s))
            && !is_under(e, me, it)
    });
    // Said while it is held: another asked for then lets go of it.
    warn(e, format!("ShellLoader: {why}"));
    if !still(e, me, loads) {
        return true;
    }
    end_hold(e, me);
    let swaps = RootSwaps::of(e);
    swaps.release(fallback);
    // In one call: a window that has the fallback still keeps it.
    let window = own_window(e, me, fallback).then_some(fallback);
    let gives = swaps.present(e, me, window);
    swaps.takes_back(me, fallback);
    set(e, me, ShellLoader::ITEM.index, Value::Object(fallback));
    if let Some(shell) = shell {
        if gives {
            e.announce_destruction(shell);
        } else {
            discard(e, shell);
        }
    }
    let item = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    if item != Some(fallback) || !say_failed(e, me, loads, why, shell_status::FALLBACK) {
        // A handler asked for another shell as it was shown, or was said.
        return true;
    }
    if !taken && let Some(item) = held.focus.filter(|it| e.is_alive(*it)) {
        force_active_focus(e, item, FocusReason::Other);
    }
    true
}

/// Shows nothing and lets go of what was shown. False where a handler of
/// that asked for another shell, which is then not the caller's to replace.
fn drop_item(e: &Engine, me: ObjectId) -> bool {
    let item = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    let Some((was, loads)) = with_run(e, me, |r| {
        r.fallback_item = None;
        (std::mem::replace(&mut r.making, true), r.loads)
    }) else {
        return false;
    };
    // In a batch, so that the window is let go of before a handler asks
    // for another, whose window would go with it otherwise.
    let let_go = e.batch(|| {
        set(e, me, ShellLoader::ITEM.index, Value::Null);
        RootSwaps::of(e).present(e, me, None)
    });
    // Who drops it shows another next: a `reload()` from a handler of this
    // would make a second under that one.
    with_run(e, me, |r| r.making = was);
    if let Some(item) = item {
        if let_go {
            // The window it showed goes once no window shows it.
            e.announce_destruction(item);
        } else {
            discard(e, item);
        }
    }
    // Asked last: the shell's own handlers of its destruction may ask too.
    with_run(e, me, |r| r.loads) == Some(loads)
}

/// A shell that is a window of its own, as a `Window` is where each has a
/// platform window: a loader that is the root of what a window shows
/// stands for it there, and the platform window shows the shell's
/// `Window` in its place.
fn present(e: &Engine, me: ObjectId, obj: ObjectId) {
    let window = own_window(e, me, obj).then_some(obj);
    RootSwaps::of(e).present(e, me, window);
}

/// Whether `obj` is a `Window` that is not among the loader's items.
pub(crate) fn own_window(e: &Engine, me: ObjectId, obj: ObjectId) -> bool {
    e.is_alive(obj)
        && e.untrack(|| e.visual_parent(obj)) != Some(me)
        && e.meta_of(obj).is_some_and(|m| m.inherits_name("Window"))
}

/// Takes `obj` off the loader now and destroys it at the next frame: a
/// handler of its own may be what is running.
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

/// Keeps the loader's and the shell's sizes together along one axis, as a
/// Loader does.
fn resize(e: &Engine, me: ObjectId, horizontal: bool) -> Result<(), EngineError> {
    let (size, implicit) = if horizontal {
        (Item::WIDTH.index, Item::IMPLICIT_WIDTH.index)
    } else {
        (Item::HEIGHT.index, Item::IMPLICIT_HEIGHT.index)
    };
    let own = e.read(me, size).to_number();
    let follows = size_follows_implicit(e, me, horizontal);
    let shown = e.read(me, ShellLoader::ITEM.index).as_object();
    // A window of its own has the platform window's size.
    let Some(shown) = shown.filter(|s| e.is_alive(*s) && is_item(e, *s) && !own_window(e, me, *s))
    else {
        return Ok(());
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
