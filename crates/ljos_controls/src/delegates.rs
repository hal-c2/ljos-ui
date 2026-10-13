//! `CheckDelegate`, `RadioDelegate` and `SwitchDelegate`
//! (QtQuick.Templates): item delegates that check, choose or switch.
//!
//! Each is an `ItemDelegate` that does what the button it is named after
//! does: the check delegate goes round its check states as a `CheckBox`
//! does, the radio delegate is exclusive among its siblings, and the
//! switch delegate has a `position` that follows `checked`. Learned by
//! running `qml6` (Qt 6.11.2), pinned by
//! `features/controls/check-delegate.feature`; documentation:
//! doc.qt.io/qt-6/qml-qtquick-controls-checkdelegate.html, -radiodelegate
//! and -switchdelegate.

use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, Value, WriteKind, qml_impl};

use crate::button::{AbstractButton, check_state, set_checked, stays_checked};
use crate::combobox::ItemDelegate;
use crate::control::Control;
use crate::util::{bind_default, check, flag, is_unset, num};

fn checkable_row(exclusive: bool) -> ItemDelegate {
    let mut row = ItemDelegate::default();
    row.button.checkable = true;
    row.button.auto_exclusive = exclusive;
    row
}

// --------------------------------------------------------- CheckDelegate

/// `CheckDelegate`.
///
/// TODO(js): `nextCheckState` is stored and not called, as on `CheckBox`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "CheckDelegate", extends = ItemDelegate)]
pub struct CheckDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
    #[qml(notify)]
    pub tristate: bool,
    /// A `Qt.CheckState`.
    #[qml(notify, enumeration = "Qt.CheckState")]
    pub check_state: i32,
    #[qml(notify)]
    pub next_check_state: Value,
}

impl Default for CheckDelegate {
    fn default() -> Self {
        Self {
            row: checkable_row(false),
            tristate: false,
            check_state: check_state::UNCHECKED,
            next_check_state: Value::Undefined,
        }
    }
}

#[qml_impl]
impl CheckDelegate {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_check(cx.engine, cx.this)
    }
}

fn init_check(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let state_p = CheckDelegate::CHECK_STATE.index;
    let checked_p = AbstractButton::CHECKED.index;
    // `checked` and `checkState` say the same thing twice; whichever the
    // author gave decides the other.
    if !is_unset(engine, obj, state_p) {
        let s = engine.untrack(|| num(engine, obj, state_p)) as i32;
        set_checked(engine, obj, s == check_state::CHECKED)?;
    } else if engine.untrack(|| flag(engine, obj, checked_p)) {
        engine.write_derived(obj, state_p, Value::Int(check_state::CHECKED))?;
    }
    engine.watch(obj, state_p, move |e, v| {
        let on = v.to_number() as i32 == check_state::CHECKED;
        check(e, "checkState", set_checked(e, obj, on));
    })?;
    engine.watch(obj, checked_p, move |e, v| {
        let on = v.to_boolean();
        let s = e.untrack(|| num(e, obj, state_p)) as i32;
        if (s == check_state::CHECKED) != on {
            let s = if on {
                check_state::CHECKED
            } else {
                check_state::UNCHECKED
            };
            check(e, "checked", set_check_state(e, obj, s));
        }
    })?;
    Ok(())
}

fn set_check_state(engine: &Engine, obj: ObjectId, s: i32) -> Result<(), EngineError> {
    let p = CheckDelegate::CHECK_STATE.index;
    if engine.is_bound(obj, p) {
        engine.write_derived(obj, p, Value::Int(s))
    } else {
        engine.write(obj, p, Value::Int(s), WriteKind::Imperative)
    }
}

/// What a click does to a check delegate: as to a check box, three states
/// go round without `toggled`.
pub(crate) fn advance(engine: &Engine, obj: ObjectId) {
    let s = engine.untrack(|| num(engine, obj, CheckDelegate::CHECK_STATE.index)) as i32;
    let tristate = engine.untrack(|| flag(engine, obj, CheckDelegate::TRISTATE.index));
    if tristate {
        check(
            engine,
            "checkState",
            set_check_state(engine, obj, (s + 1) % 3),
        );
        return;
    }
    let next = if s == check_state::CHECKED {
        check_state::UNCHECKED
    } else {
        check_state::CHECKED
    };
    if next == check_state::UNCHECKED && stays_checked(engine, obj) {
        return;
    }
    check(engine, "checkState", set_check_state(engine, obj, next));
    check(engine, "toggled", engine.emit_by_name(obj, "toggled", &[]));
}

// --------------------------------------------------------- RadioDelegate

/// `RadioDelegate`: checkable, and exclusive among the buttons beside it.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "RadioDelegate", extends = ItemDelegate)]
pub struct RadioDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
}

impl Default for RadioDelegate {
    fn default() -> Self {
        Self {
            row: checkable_row(true),
        }
    }
}

// -------------------------------------------------------- SwitchDelegate

/// `SwitchDelegate`. Its handle follows `checked`, and is dragged as a
/// `Switch`'s is (the drag is `button.rs`'s, for both).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "SwitchDelegate", extends = ItemDelegate)]
pub struct SwitchDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
    /// 0 off, 1 on.
    #[qml(notify)]
    pub position: f64,
    /// `position`, from the left: the other way round when mirrored.
    #[qml(notify, readonly)]
    pub visual_position: f64,
}

impl Default for SwitchDelegate {
    fn default() -> Self {
        Self {
            row: checkable_row(false),
            position: 0.0,
            visual_position: 0.0,
        }
    }
}

#[qml_impl]
impl SwitchDelegate {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, obj) = (cx.engine, cx.this);
        bind_default(engine, obj, SwitchDelegate::POSITION.index, move |e| {
            Value::Real(f64::from(u8::from(flag(
                e,
                obj,
                AbstractButton::CHECKED.index,
            ))))
        })?;
        engine.bind_initial(obj, SwitchDelegate::VISUAL_POSITION.index, move |e| {
            let p = num(e, obj, SwitchDelegate::POSITION.index);
            Value::Real(if flag(e, obj, Control::MIRRORED.index) {
                1.0 - p
            } else {
                p
            })
        })
    }
}
