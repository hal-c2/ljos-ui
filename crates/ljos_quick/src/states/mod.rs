//! States: `State` and what goes in one, `StateGroup`, and the `Binding`
//! element.
//!
//! A state is a list of changes. Entering it puts each change on the
//! override layer of the property it is for ([`Engine::push_override`]),
//! and leaving it takes them off again, so whatever the property had
//! before (a value, a binding that went on changing underneath) is there
//! when the state is left. Nothing is saved and written back.
//!
//! - [`types`]: the QML objects. They hold what QML wrote.
//! - [`group`]: one per item (or `StateGroup`) that has states: which
//!   state it is in, what that state put where, and the change from one
//!   state to the next, with the transition that shows it
//!   ([`crate::animation`]).
//! - [`binding`]: `Binding`, one override that lasts while `when` is true.
//!
//! Documentation: doc.qt.io/qt-6/qtquick-statesanimations-states.html,
//! qml-qtquick-state.html, qml-qtquick-propertychanges.html,
//! qml-qtqml-binding.html.

pub(crate) mod binding;
pub(crate) mod group;
pub(crate) mod types;

use std::cell::Cell;

use ljos_engine::{Engine, QmlType};

use crate::item::is_a;

pub use binding::Binding;
pub use types::{
    AnchorChanges, AnchorChangesAnchors, ParentChange, PropertyChanges, State, StateChangeScript,
    StateGroup,
};

/// Holds back the states of `owner` until [`start_states`]: for a host
/// of content, whose children are given their parent when it completes.
pub fn defer_states(engine: &Engine, owner: ljos_engine::ObjectId) {
    group::defer(engine, owner);
}

/// Gives `owner` the states that [`defer_states`] held back.
pub fn start_states(engine: &Engine, owner: ljos_engine::ObjectId) {
    group::start(engine, owner);
}

#[derive(Default)]
struct Registered(Cell<bool>);

/// Hooks the state types into the engine. Once per engine; more calls do
/// nothing.
pub(crate) fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        State, StateGroup, PropertyChanges, AnchorChanges, ParentChange, StateChangeScript,
    ]));
    // `Binding` is a QtQml type. It is here because it is the same thing
    // as a state with one change, and nothing below this crate has the
    // frame its `delayed` waits for.
    engine.register_module(ljos_engine::module!("QtQml" 2.0 => [Binding]));
    // What a PropertyChanges is given are properties of its target.
    engine.register_foreign_members(PropertyChanges::static_meta(), |e, obj, member| {
        let _ = e.with_mut::<PropertyChanges, _>(obj, |pc| pc.members.push(member));
    });
    // `script: ...` is a statement to run when the state is entered.
    engine.register_script_property(
        StateChangeScript::static_meta(),
        StateChangeScript::SCRIPT.index,
    );
    // `Binding on width { }`.
    engine.on_value_source(|e, source, target, prop| {
        is_a::<Binding>(e, source) && binding::aim(e, source, target, prop)
    });
}
