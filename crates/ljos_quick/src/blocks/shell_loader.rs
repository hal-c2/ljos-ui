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
//! A resource fault (the script heap ran out, a script was interrupted)
//! is not the shell's own: the fallback stands in for it while the loader
//! waits as a Loader does ([`Backoff`]: a second, twice as long each time
//! the shell fails again soon after, up to a minute), and it then tries
//! the source again, for as long as it takes.
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

use std::time::Duration;

use ljos_engine::backoff::{Backoff, seconds};
use ljos_engine::{
    Atom, Boundary, BoundaryConfig, Cx, Engine, EngineError, FaultReport, LogLevel, ObjectId,
    PropIndex, QmlObject, Request, Url, Value, WriteKind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, Restart, TimerId};

use super::contribution::text;
use crate::item::{Item, is_item, size_follows_implicit, write_computed};
use crate::roots::RootSwaps;
use crate::tree;

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
    /// while it is.
    #[qml(notify, readonly)]
    pub error: Atom,
    /// The shell shown.
    #[qml(notify, readonly)]
    pub item: Option<ObjectId>,
    /// [`shell_status`].
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
    fallback_item: Option<ObjectId>,
    /// Faults up to this one were dealt with while a shell was made; the
    /// boundary hears of them only afterwards.
    handled: u64,
    /// When the source is tried again after a resource fault.
    backoff: Backoff,
    /// The try [`Run::backoff`] waits for.
    waiting: Option<TimerId>,
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
        e.with(me, |l: &ShellLoader| {
            !l.run.making_fallback && l.run.fallback_item != Some(child)
        })
        .unwrap_or(false)
    }

    fn stop(&self, e: &Engine, me: ObjectId, report: &FaultReport) {
        let handled = with_run(e, me, |r| r.handled).unwrap_or(u64::MAX);
        if report.seq <= handled || failed(e, me) {
            return;
        }
        drop_item(e, me);
        let why = describe(report);
        warn(e, format!("ShellLoader: {why}"));
        if show_fallback(e, me, why) || is_resource(&report.fault) {
            again_after(e, me);
        }
    }

    fn start(&self, _: &Engine, _: ObjectId) -> Result<(), Fault> {
        // Never asked of a temporary child.
        Ok(())
    }

    fn failed(&self, e: &Engine, me: ObjectId) -> bool {
        failed(e, me)
    }
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

/// Shows the source's shell, or the fallback's.
fn load(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) || with_run(e, me, |r| r.completed) != Some(true) {
        return;
    }
    if let Some(Some(timer)) = with_run(e, me, |r| r.waiting.take()) {
        e.cancel_call(timer);
    }
    drop_item(e, me);
    let source = url(e, me, ShellLoader::SOURCE.index);
    if source.is_empty() {
        if show_fallback(e, me, String::new()) {
            again_after(e, me);
        }
        return;
    }
    match make(e, me, &source) {
        Ok(obj) => {
            set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
            present(e, me, obj);
            set(e, me, ShellLoader::STATUS.index, shell_status::READY.into());
            set(e, me, ShellLoader::ERROR.index, "".into());
        }
        Err(Failed { why, resource }) => {
            warn(e, format!("ShellLoader: {why}"));
            if show_fallback(e, me, why) || resource {
                again_after(e, me);
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
/// wait is over, the fallback shown meanwhile.
fn again_after(e: &Engine, me: ObjectId) {
    let now = e.now();
    let Some(wait) = with_run(e, me, |r| r.backoff.fault(now)) else {
        return;
    };
    // A full heap may have room once the garbage is collected.
    e.collect_garbage();
    warn(
        e,
        format!("ShellLoader: trying the source again in {}", seconds(wait)),
    );
    let timer = e.call_at(now + wait, move |e| {
        let tried = with_run(e, me, |r| {
            r.waiting = None;
            r.backoff.trying(e.now());
        });
        if tried.is_some() {
            load(e, me);
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
/// call raises the backoff.
fn show_fallback(e: &Engine, me: ObjectId, why: String) -> bool {
    let fallback = url(e, me, ShellLoader::FALLBACK.index);
    if fallback.is_empty() {
        let status = if why.is_empty() {
            shell_status::NULL
        } else {
            shell_status::ERROR
        };
        set(e, me, ShellLoader::ERROR.index, why.as_str().into());
        set(e, me, ShellLoader::STATUS.index, status.into());
        return false;
    }
    with_run(e, me, |r| r.making_fallback = true);
    let made = make(e, me, &fallback);
    let mut again = false;
    match made {
        Ok(obj) => {
            with_run(e, me, |r| r.fallback_item = Some(obj));
            set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
            present(e, me, obj);
            set(e, me, ShellLoader::ERROR.index, why.as_str().into());
            set(
                e,
                me,
                ShellLoader::STATUS.index,
                shell_status::FALLBACK.into(),
            );
        }
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
            set(e, me, ShellLoader::ERROR.index, both.as_str().into());
            set(e, me, ShellLoader::STATUS.index, shell_status::ERROR.into());
        }
    }
    with_run(e, me, |r| r.making_fallback = false);
    // Nothing could be shown for want of room: the source is tried again,
    // and the fallback with it.
    again
}

/// Makes the shell at `url`, or says why it could not: it did not
/// compile, or something in it faulted while it was made.
fn make(e: &Engine, me: ObjectId, url: &str) -> Result<ObjectId, Failed> {
    let (made, filed) = e.noting_faults(|| {
        e.make(
            Request::url(e.resolve_url(me, url))
                .parent(Some(me))
                .in_scope(me),
        )
        .map(|(shell, _)| shell)
    });
    let after = last_fault(e);
    with_run(e, me, |r| r.handled = after);
    let obj = made.map_err(|err| Failed {
        resource: matches!(
            err,
            EngineError::JsOutOfMemory(_) | EngineError::JsInterrupted(_)
        ),
        why: err.to_string(),
    })?;
    if let Some(report) = fault_in(e, obj, &filed) {
        let failed = Failed {
            why: describe(report),
            resource: is_resource(&report.fault),
        };
        discard(e, obj);
        return Err(failed);
    }
    // A binding that throws while the shell is made is a fault only if
    // it still throws once the creation around the loader is complete,
    // as an application starts: the shell is judged again then, by those
    // faults alone. One raised after it was made, as by a handler the
    // rest of the tree sets off as it completes, is the boundary's.
    e.after_creation_told(move |e, told| judge_again(e, me, obj, told));
    Ok(obj)
}

/// The first of `faults` raised in `obj` or what it owns.
fn fault_in<'a>(e: &Engine, obj: ObjectId, faults: &'a [FaultReport]) -> Option<&'a FaultReport> {
    faults
        .iter()
        .find(|r| r.origin.is_some_and(|o| within(e, o, obj)))
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
    let why = describe(report);
    if with_run(e, me, |r| r.fallback_item) == Some(Some(obj)) {
        warn(e, format!("ShellLoader: fallback: {why}"));
        let first = text(&e.untrack(|| e.read(me, ShellLoader::ERROR.index)));
        let both = if first.is_empty() {
            format!("fallback: {why}")
        } else {
            format!("{first}\nfallback: {why}")
        };
        drop_item(e, me);
        set(e, me, ShellLoader::ERROR.index, both.as_str().into());
        set(e, me, ShellLoader::STATUS.index, shell_status::ERROR.into());
        if is_resource(&report.fault) {
            again_after(e, me);
        }
        return;
    }
    drop_item(e, me);
    warn(e, format!("ShellLoader: {why}"));
    let resource = is_resource(&report.fault);
    if show_fallback(e, me, why) || resource {
        again_after(e, me);
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

/// Takes away the shell shown.
fn drop_item(e: &Engine, me: ObjectId) {
    let item = e
        .untrack(|| e.read(me, ShellLoader::ITEM.index))
        .as_object();
    with_run(e, me, |r| r.fallback_item = None);
    set(e, me, ShellLoader::ITEM.index, Value::Null);
    let let_go = RootSwaps::of(e).present(e, me, None);
    if let Some(item) = item {
        if let_go {
            // The window it showed goes once no window shows it.
            e.announce_destruction(item);
        } else {
            discard(e, item);
        }
    }
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
fn own_window(e: &Engine, me: ObjectId, obj: ObjectId) -> bool {
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
