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
    if let f @ Value::Js(_) =
        engine.untrack(|| engine.read(obj, CheckDelegate::NEXT_CHECK_STATE.index))
    {
        if let Some(v) = engine.call_js(&f, &[]) {
            let next = (v.to_number() as i32).clamp(0, 2);
            check(engine, "checkState", set_check_state(engine, obj, next));
            if !tristate && next != s {
                check(engine, "toggled", engine.emit_by_name(obj, "toggled", &[]));
            }
        }
        return;
    }
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

// ------------------------------------------------------ TreeViewDelegate

/// `TreeViewDelegate`: the stock delegate of a `TreeView`. The view
/// writes where the delegate's row is in the tree (`depth`, `expanded`,
/// `hasChildren`, `isTreeNode`) and whether its cell is current or
/// selected; the style moves the text right by `depth` levels of
/// `indentation`, after an indicator that expands and collapses the row.
/// Documentation: doc.qt.io/qt-6/qml-qtquick-controls-treeviewdelegate.html;
/// pinned by `features/controls/tree-view-delegate.feature`.
///
/// The properties the view gives are required in Qt; here they are
/// plain properties the view writes before the delegate's bindings run.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "TreeViewDelegate", extends = ItemDelegate)]
pub struct TreeViewDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
    /// How far right each level of depth moves the row.
    #[qml(notify)]
    pub indentation: f64,
    #[qml(notify)]
    pub left_margin: f64,
    #[qml(notify)]
    pub right_margin: f64,
    #[qml(notify, rename = "treeView", object = "TreeView")]
    pub tree_view: Option<ObjectId>,
    #[qml(notify)]
    pub is_tree_node: bool,
    #[qml(notify)]
    pub has_children: bool,
    #[qml(notify)]
    pub expanded: bool,
    #[qml(notify)]
    pub depth: i32,
    #[qml(notify)]
    pub current: bool,
    #[qml(notify)]
    pub selected: bool,
    #[qml(notify)]
    pub editing: bool,
}
