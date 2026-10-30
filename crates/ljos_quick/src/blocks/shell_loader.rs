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
//! It is not a `Loader`: a Loader only logs why it could not load, and it
//! holds one URL. The two make their item the same way.
//!
//! Under `--watch` a saved file is reloaded by making the application's
//! roots again (ARCHITECTURE §12.1), and so the loader tries its source
//! again. A shell that was shown and that an edit breaks is not replaced
//! by the fallback: a reload that does not compile keeps the generation
//! that runs, and says why in the reload error, not in `error`.

use std::time::Duration;

use ljos_engine::{
    Atom, Boundary, BoundaryConfig, Cx, Engine, EngineError, FaultReport, LogLevel, ObjectId,
    PropIndex, QmlObject, Url, Value, WriteKind, qml_impl,
};
use ljos_runtime::{Fault, Intensity, Restart};

use super::contribution::text;
use crate::item::{Item, is_item, size_follows_implicit, write_computed};
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
        show_fallback(e, me, why);
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
    drop_item(e, me);
    let source = url(e, me, ShellLoader::SOURCE.index);
    if source.is_empty() {
        show_fallback(e, me, String::new());
        return;
    }
    match make(e, me, &source) {
        Ok(obj) => {
            set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
            set(e, me, ShellLoader::STATUS.index, shell_status::READY.into());
            set(e, me, ShellLoader::ERROR.index, "".into());
        }
        Err(why) => {
            warn(e, format!("ShellLoader: {why}"));
            show_fallback(e, me, why);
        }
    }
}

/// Shows the fallback's shell in place of the source's, which did not
/// work for `why` (empty when there is no source).
fn show_fallback(e: &Engine, me: ObjectId, why: String) {
    let fallback = url(e, me, ShellLoader::FALLBACK.index);
    if fallback.is_empty() {
        let status = if why.is_empty() {
            shell_status::NULL
        } else {
            shell_status::ERROR
        };
        set(e, me, ShellLoader::ERROR.index, why.as_str().into());
        set(e, me, ShellLoader::STATUS.index, status.into());
        return;
    }
    with_run(e, me, |r| r.making_fallback = true);
    let made = make(e, me, &fallback);
    match made {
        Ok(obj) => {
            with_run(e, me, |r| r.fallback_item = Some(obj));
            set(e, me, ShellLoader::ITEM.index, Value::Object(obj));
            set(e, me, ShellLoader::ERROR.index, why.as_str().into());
            set(
                e,
                me,
                ShellLoader::STATUS.index,
                shell_status::FALLBACK.into(),
            );
        }
        Err(also) => {
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
}

/// Makes the shell at `url`, or says why it could not: it did not
/// compile, or something in it faulted while it was made.
fn make(e: &Engine, me: ObjectId, url: &str) -> Result<ObjectId, String> {
    let before = last_fault(e);
    let made = e.create_from_url_in(&e.resolve_url(me, url), Some(me), Vec::new(), Some(me));
    let after = last_fault(e);
    with_run(e, me, |r| r.handled = after);
    let obj = made.map_err(|err| err.to_string())?;
    let fault = e
        .faults()
        .into_iter()
        .find(|r| r.seq > before && r.origin.is_some_and(|o| within(e, o, obj)));
    if let Some(report) = fault {
        discard(e, obj);
        return Err(describe(&report));
    }
    Ok(obj)
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
    if let Some(item) = item {
        discard(e, item);
    }
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
    let Some(shown) = shown.filter(|s| e.is_alive(*s) && is_item(e, *s)) else {
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
