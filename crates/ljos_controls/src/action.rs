//! `Action` (QtQuick.Controls): something the user can do, which several
//! buttons and menu items can stand for.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-action.html; pinned
//! by `features/controls/action.feature`, `action-shortcut.feature` and
//! `action-group.feature`.
//!
//! A `shortcut` is a `Shortcut` (QtQuick) the action owns, made the first
//! time the action has one, so that an action without a shortcut costs
//! the key handling nothing.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_quick::keys::Shortcut;

use crate::button::Icon;
use crate::util::check;

/// `Action`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Action", extends = QtObject)]
pub struct Action {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub text: Atom,
    #[qml(group)]
    pub icon: Icon,
    #[qml(notify)]
    pub checkable: bool,
    #[qml(notify)]
    pub checked: bool,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub shortcut: Value,
}

impl Default for Action {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            text: Atom::EMPTY,
            icon: Icon::default(),
            checkable: false,
            checked: false,
            enabled: true,
            shortcut: Value::Undefined,
        }
    }
}

fn source_of(v: &Value) -> Value {
    match v.as_object() {
        Some(o) => Value::Object(o),
        None => Value::Null,
    }
}

#[qml_impl]
impl Action {
    /// The action was done, by `source` (a button) or by a script.
    #[qml(signal)]
    fn triggered(source: Option<ObjectId>);
    /// A checkable action was checked or unchecked by being triggered.
    #[qml(signal)]
    fn toggled(source: Option<ObjectId>);

    /// Does the action, unless it is disabled of its own (a disabled group
    /// does not stop a script, on qml6): a checkable one changes its
    /// `checked` first.
    #[qml(invokable)]
    fn trigger(cx: &Cx, source: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        if !crate::action_group::own_enabled(e, cx.this) {
            return Ok(());
        }
        let source = source_of(&source);
        // The checked action of an exclusive group stays checked.
        if e.untrack(|| cx.get(Action::CHECKABLE))?
            && !crate::action_group::keeps_checked(e, cx.this)
        {
            let on = e.untrack(|| cx.get(Action::CHECKED))?;
            cx.set(Action::CHECKED, !on)?;
            e.emit_by_name(cx.this, "toggled", std::slice::from_ref(&source))?;
        }
        if e.is_alive(cx.this) {
            e.emit_by_name(cx.this, "triggered", std::slice::from_ref(&source))?;
        }
        if e.is_alive(cx.this) {
            crate::action_group::triggered(e, cx.this)?;
        }
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, this) = (cx.engine, cx.this);
        let made = Rc::new(Cell::new(false));
        let wanted = move |e: &Engine| {
            !matches!(
                e.untrack(|| e.read(this, Action::SHORTCUT.index)),
                Value::Undefined
            )
        };
        if wanted(e) {
            made.set(true);
            make_shortcut(e, this)?;
        }
        e.watch(this, Action::SHORTCUT.index, move |e, _| {
            if !made.get() && wanted(e) {
                made.set(true);
                check(e, "shortcut", make_shortcut(e, this));
            }
        })?;
        Ok(())
    }

    /// Checks or unchecks a checkable action that is enabled of its own.
    #[qml(invokable)]
    fn toggle(cx: &Cx, source: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        let usable = crate::action_group::own_enabled(e, cx.this)
            && e.untrack(|| cx.get(Action::CHECKABLE))?;
        if !usable {
            return Ok(());
        }
        let on = e.untrack(|| cx.get(Action::CHECKED))?;
        cx.set(Action::CHECKED, !on)?;
        e.emit_by_name(cx.this, "toggled", &[source_of(&source)])
    }
}

/// The `Shortcut` that stands for the action's `shortcut`: it follows the
/// action's sequence and `enabled`, and triggers the action, which is its
/// own source (as on qml6).
fn make_shortcut(e: &Engine, action: ObjectId) -> Result<(), EngineError> {
    let s = e.create_native(Shortcut::default(), Some(action))?;
    e.bind(s, Shortcut::SEQUENCE.index, move |e| {
        e.read(action, Action::SHORTCUT.index)
    })?;
    e.bind(s, Shortcut::ENABLED.index, move |e| {
        e.read(action, Action::ENABLED.index)
    })?;
    e.connect_by_name(s, "activated", move |e, _| {
        if e.is_alive(action) {
            let r = e.invoke_by_name(action, "trigger", &[Value::Object(action)]);
            check(e, "shortcut", r.map(|_| ()));
        }
    })?;
    e.complete(s)
}
