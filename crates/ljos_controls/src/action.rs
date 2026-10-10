//! `Action` (QtQuick.Controls): something the user can do, which several
//! buttons and menu items can stand for.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-action.html; pinned
//! by `features/controls/action.feature`.
//!
//! TODO(keys): `shortcut` is stored; no key sequence triggers an action
//! yet.

use ljos_engine::{Atom, Cx, EngineError, ObjectId, QmlObject, QtObject, Value, qml_impl};

use crate::button::Icon;

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

    /// Does the action, unless it is disabled: a checkable one changes
    /// its `checked` first.
    #[qml(invokable)]
    fn trigger(cx: &Cx, source: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        if !e.untrack(|| cx.get(Action::ENABLED))? {
            return Ok(());
        }
        let source = source_of(&source);
        if e.untrack(|| cx.get(Action::CHECKABLE))? {
            let on = e.untrack(|| cx.get(Action::CHECKED))?;
            cx.set(Action::CHECKED, !on)?;
            e.emit_by_name(cx.this, "toggled", std::slice::from_ref(&source))?;
        }
        if e.is_alive(cx.this) {
            e.emit_by_name(cx.this, "triggered", std::slice::from_ref(&source))?;
        }
        Ok(())
    }

    /// Checks or unchecks a checkable, enabled action.
    #[qml(invokable)]
    fn toggle(cx: &Cx, source: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        let usable = e.untrack(|| -> Result<bool, EngineError> {
            Ok(cx.get(Action::ENABLED)? && cx.get(Action::CHECKABLE)?)
        })?;
        if !usable {
            return Ok(());
        }
        let on = e.untrack(|| cx.get(Action::CHECKED))?;
        cx.set(Action::CHECKED, !on)?;
        e.emit_by_name(cx.this, "toggled", &[source_of(&source)])
    }
}
