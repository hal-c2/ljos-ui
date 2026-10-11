//! `focusPolicy` and `activeFocusOnTab`, which say one thing between
//! them: whether the Tab key stops at an item.
//!
//! Measured on Qt 6.11 (qml6): the policy reads as what was written with
//! `Qt.TabFocus` added while `activeFocusOnTab` is true, and a policy
//! that is written sets `activeFocusOnTab` to whether it has that bit. A
//! `TextField` has the policy `Qt.TabFocus` that way without one being
//! written. Qt 6.11 has `focusPolicy` on every `Item`; here the controls
//! and the three text types of this crate have it.
//!
//! Pinned by `features/controls/focus-policy.feature`.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, EngineError, ObjectId, PropIndex, Value};
use ljos_quick::Item;

use crate::control::focus_policy;
use crate::util::{check, flag, is_unset, num, set_computed};

/// Ties the policy in `policy` to `activeFocusOnTab`.
///
/// `own_stop` is for a type that is a Tab stop without a policy saying so
/// (a text field): its `activeFocusOnTab` stands while no policy is
/// written.
pub(crate) fn link_policy(
    engine: &Engine,
    obj: ObjectId,
    policy: PropIndex,
    own_stop: bool,
) -> Result<(), EngineError> {
    let stop = Item::ACTIVE_FOCUS_ON_TAB.index;
    let written = engine.untrack(|| num(engine, obj, policy)) as i32;
    // What was written, without the bit `activeFocusOnTab` adds.
    let stored = Rc::new(Cell::new(written));

    // What the author wrote for `activeFocusOnTab` stands whatever the
    // policy the item starts with.
    if is_unset(engine, obj, stop) && !(own_stop && written == 0) {
        engine.write_derived(obj, stop, Value::Bool(written & focus_policy::TAB != 0))?;
    }
    let shown = move |e: &Engine, stored: i32| {
        let on = e.untrack(|| flag(e, obj, stop));
        stored | if on { focus_policy::TAB } else { 0 }
    };
    let show = move |e: &Engine, stored: i32| {
        let v = shown(e, stored);
        if e.untrack(|| num(e, obj, policy)) as i32 != v {
            // Not the author's: the policy does not read as assigned.
            check(
                e,
                "focusPolicy",
                e.write_derived(obj, policy, Value::Int(v)),
            );
        }
    };
    show(engine, written);

    let st = stored.clone();
    engine.watch(obj, policy, move |e, v| {
        let v = v.to_number() as i32;
        if v == shown(e, st.get()) {
            // What `show` wrote.
            return;
        }
        // The author's. Writing what is stored again changes nothing, as
        // in Qt: the bit `activeFocusOnTab` adds stays.
        if v != st.get() {
            st.set(v);
            let on = Value::Bool(v & focus_policy::TAB != 0);
            check(e, "activeFocusOnTab", set_computed(e, obj, stop, on));
        }
        show(e, st.get());
    })?;
    engine.watch(obj, stop, move |e, _| show(e, stored.get()))?;
    Ok(())
}
