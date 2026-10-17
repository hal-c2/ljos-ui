//! Asking a model for more rows when a view reaches its end.
//!
//! A pass that reaches the last row asks the model whether it has more
//! (`can_fetch_more`) and, after the pass, has it read them
//! (`fetch_more`). The call waits for the pass to end because a model may
//! insert its rows at once, and rows that change under a pass would be
//! laid out half old and half new. It is a due-now timer, so it runs on
//! the engine's next turn of timers. The passes before it runs ask
//! nothing more of the model: one call for however many passes reached
//! the end.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, ItemModel, ObjectId};
use rustc_hash::FxHashSet;

/// The views with a call on the way.
#[derive(Default)]
struct Asked(RefCell<FxHashSet<ObjectId>>);

/// Has `model` read more rows for `view` soon, unless it has none or the
/// view already asked. `current` says, when the call runs, whether the
/// view still shows `model`.
pub(crate) fn soon(
    e: &Engine,
    view: ObjectId,
    model: Rc<dyn ItemModel>,
    current: fn(&Engine, ObjectId) -> Option<Rc<dyn ItemModel>>,
) {
    let asked = e.extension::<Asked>();
    if asked.0.borrow().contains(&view) || !model.can_fetch_more() {
        return;
    }
    asked.0.borrow_mut().insert(view);
    e.call_at(e.now(), move |e| {
        e.extension::<Asked>().0.borrow_mut().remove(&view);
        let same = current(e, view).is_some_and(|m| Rc::ptr_eq(&m, &model));
        if e.is_alive(view) && same && model.can_fetch_more() {
            model.fetch_more(e);
        }
    });
}
