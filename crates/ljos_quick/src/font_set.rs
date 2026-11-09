//! The set of fonts as something a binding follows, and the one place that
//! says it changed.
//!
//! What a font resolves to (`FontInfo`) depends on the fonts there are: a
//! font registered later, or another text engine, can be the face a family
//! resolves to from then on. Every change to the set goes through
//! [`changed`], which tells every engine on the thread that follows it
//! before it returns. A `FontLoader` tells it inside the batch that
//! publishes its result, so a handler of its `status` sees the face it
//! loaded, not the one there was before.
//!
//! Generations ([`ljos_text::TextEngine::generation`]) never repeat, in one
//! engine or across engines, so "the fonts the followers were told of"
//! names one engine's set of fonts and an engine that replaces another is
//! never taken for it.

use std::cell::{Cell, RefCell};

use ljos_engine::{Engine, LogLevel, ObjectId, QmlObject, Value, WeakEngine};

use crate::item::write_computed;
use crate::text::{text_engine, text_engine_unless_coming};

thread_local! {
    /// The engines of this thread that follow the set of fonts.
    static FOLLOWERS: RefCell<Vec<WeakEngine>> = const { RefCell::new(Vec::new()) };
    /// The text engine generation they were last told of; `None` while
    /// the engine is still being made on another thread.
    static TOLD: Cell<Option<u64>> = const { Cell::new(None) };
    /// How many changes they were told of, which their [`FontSet`] says.
    static CHANGES: Cell<u64> = const { Cell::new(0) };
}

/// What a binding that follows the set of fonts reads: one per engine,
/// made by the first such binding.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "FontSet", uncreatable)]
pub struct FontSet {
    /// How many times the set of fonts changed on this thread.
    #[qml(notify, readonly)]
    pub changes: f64,
}

#[derive(Default)]
struct FontSetOf(Cell<Option<ObjectId>>);

/// The engines that follow the set of fonts and are still alive.
fn followers() -> Vec<Engine> {
    FOLLOWERS.with(|f| {
        let mut f = f.borrow_mut();
        let alive: Vec<Engine> = f.iter().filter_map(WeakEngine::upgrade).collect();
        f.retain(|w| w.upgrade().is_some());
        alive
    })
}

/// Makes the binding being worked out follow the set of fonts.
pub(crate) fn follow(e: &Engine) {
    let of = e.extension::<FontSetOf>();
    let set = match of.0.get().filter(|o| e.is_alive(*o)) {
        Some(set) => set,
        None => {
            let changes = CHANGES.get() as f64;
            let Ok(set) = e.create_native_incomplete(FontSet { changes }, None) else {
                return;
            };
            of.0.set(Some(set));
            let others = followers();
            // The first follower follows the fonts there are now; a later
            // one takes the generation the others were told of, so that a
            // change none was told of yet is told to all. An engine still
            // being made is not waited for: following is not a use of it,
            // and it says its generation when it is installed (`arrived`).
            if others.is_empty()
                && let Some(te) = text_engine_unless_coming()
            {
                TOLD.set(Some(te.borrow().generation()));
            }
            if !others.iter().any(|o| Engine::ptr_eq(o, e)) {
                FOLLOWERS.with(|f| f.borrow_mut().push(e.downgrade()));
            }
            set
        }
    };
    let _ = e.read(set, FontSet::CHANGES.index);
}

/// Tells every engine that follows the set of fonts that it changed, to
/// `now` (the text engine's generation; `None` for one still being made).
/// What follows it is worked out again when it is next read, so nothing
/// that reads it after this returns sees the fonts there were before.
///
/// Every engine is told before any effect runs: the writes happen inside
/// the batches of all of them, and their effects run as the batches close.
/// An effect of one engine that reads another's `FontInfo` then sees the
/// fonts there are now, not what that engine had cached. No borrow of the
/// text engine or of the followers is held while effects run, so one may
/// lay text out, and one may change the fonts again: that call is told to
/// every engine in turn, and an engine whose batch is still open runs its
/// effects once, when it closes.
pub(crate) fn changed(now: Option<u64>) {
    TOLD.set(now);
    let n = CHANGES.get() + 1;
    CHANGES.set(n);
    let engines = followers();
    batched(&engines, || {
        for e in &engines {
            let Some(set) = e.extension::<FontSetOf>().0.get() else {
                continue;
            };
            if e.is_alive(set)
                && let Err(err) =
                    write_computed(e, set, FontSet::CHANGES.index, Value::Real(n as f64))
            {
                e.log(LogLevel::Warning, "qml", format!("{err}"));
            }
        }
    });
}

/// Runs `f` inside the batch of every one of `engines`, so that their
/// effects wait until it has returned.
fn batched(engines: &[Engine], f: impl FnOnce()) {
    match engines.split_first() {
        Some((e, rest)) => e.batch(|| batched(rest, f)),
        None => f(),
    }
}

/// [`changed`], for a change to the UI thread's text engine itself (a font
/// it registered).
pub(crate) fn fonts_changed() {
    // Out of the borrow before anyone is told: what they run lays text out.
    let generation = text_engine().borrow().generation();
    changed(Some(generation));
}

/// The text engine was found at `generation`, by something that looks on
/// frames that run anyway (a scene): a host that registered a font on the
/// engine itself is followed from then on. Nothing when the followers were
/// told of it; what they were told is the measure, not what the one looking
/// saw last, which may be a change no one was told of.
pub(crate) fn seen(generation: u64) {
    if TOLD.get() != Some(generation) {
        changed(Some(generation));
    }
}

/// The engine made on another thread was installed, at `generation`: what
/// follows the fonts was told when it was asked for, and reads it next.
pub(crate) fn arrived(generation: u64) {
    TOLD.set(Some(generation));
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::text::{set_text_engine_later, text_engine_in_use};

    /// A binding that follows the fonts while the text engine is still
    /// being read on another thread does not wait for it: only a use of
    /// the engine does.
    #[test]
    fn following_the_fonts_while_the_engine_is_being_made_does_not_wait_for_it() {
        let (go, gate) = mpsc::channel::<()>();
        // Held until the test lets it go. A follow that joined it would
        // wait out the timeout and find the engine installed.
        set_text_engine_later(std::thread::spawn(move || {
            let _ = gate.recv_timeout(Duration::from_secs(5));
            ljos_text::TextEngine::new()
        }));
        let e = Engine::new();
        crate::register(&e);
        follow(&e);
        assert!(
            text_engine_in_use().is_none(),
            "follow waited for the engine being made"
        );
        assert_eq!(TOLD.get(), None, "told of an engine that is not there");

        go.send(()).unwrap();
        let generation = text_engine().borrow().generation();
        assert_eq!(TOLD.get(), Some(generation), "the engine that arrived");
    }
}
