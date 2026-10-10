//! `ButtonGroup` (QtQuick.Controls): buttons of which, usually, one is
//! checked.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-buttongroup.html;
//! pinned by `features/controls/button-group.feature`.

use ljos_engine::{
    Cx, Engine, EngineError, ObjectId, QmlList, QmlObject, QtObject, Value, qml_impl,
};

use crate::button::{AbstractButton, check_state, set_checked};
use crate::util::{check, flag, is_a, object, set_computed};

/// `ButtonGroup`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ButtonGroup",
    extends = QtObject,
    attached = ButtonGroupAttached
)]
pub struct ButtonGroup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, list, append = group_append)]
    pub buttons: QmlList,
    #[qml(notify)]
    pub checked_button: Option<ObjectId>,
    #[qml(notify)]
    pub exclusive: bool,
    /// A `Qt.CheckState`: none, some or all of the buttons are checked.
    #[qml(notify)]
    pub check_state: i32,
    /// The checked button as the group last settled it, so that a change
    /// of `checkedButton` knows which button to uncheck.
    #[qml(skip)]
    current: Option<ObjectId>,
    /// The `checkState` the group computed, so that an assignment to the
    /// property can be told from the group's own update.
    #[qml(skip)]
    computed_state: i32,
}

impl Default for ButtonGroup {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            buttons: QmlList::default(),
            checked_button: None,
            exclusive: true,
            check_state: check_state::UNCHECKED,
            current: None,
            computed_state: check_state::UNCHECKED,
        }
    }
}

/// What `ButtonGroup.group: someGroup` attaches to a button.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ButtonGroupAttached")]
pub struct ButtonGroupAttached {
    #[qml(notify)]
    pub group: Option<ObjectId>,
}

fn group_append(cx: &Cx, button: ObjectId) -> Result<(), EngineError> {
    add(cx.engine, cx.this, button)
}

#[qml_impl]
impl ButtonGroup {
    /// A button of the group was clicked.
    #[qml(signal)]
    fn clicked(button: Option<ObjectId>);

    #[qml(invokable, name = "addButton")]
    fn add_button(cx: &Cx, button: Option<ObjectId>) -> Result<(), EngineError> {
        match button {
            Some(b) => add(cx.engine, cx.this, b),
            None => Ok(()),
        }
    }

    #[qml(invokable, name = "removeButton")]
    fn remove_button(cx: &Cx, button: Option<ObjectId>) -> Result<(), EngineError> {
        match button {
            Some(b) => remove(cx.engine, cx.this, b),
            None => Ok(()),
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, group) = (cx.engine, cx.this);
        let wanted = e.untrack(|| object(e, group, ButtonGroup::CHECKED_BUTTON.index));
        if wanted.is_some() {
            set_current(e, group, wanted)?;
        }
        e.watch(group, ButtonGroup::CHECKED_BUTTON.index, move |e, v| {
            let b = v.as_object().filter(|b| e.is_alive(*b));
            check(e, "checkedButton", set_current(e, group, b));
        })?;
        e.watch(group, ButtonGroup::CHECK_STATE.index, move |e, v| {
            check(
                e,
                "checkState",
                state_assigned(e, group, v.to_number() as i32),
            );
        })?;
        e.watch(group, ButtonGroup::EXCLUSIVE.index, move |e, _| {
            check(e, "exclusive", recompute(e, group));
        })?;
        recompute(e, group)
    }
}

fn members(engine: &Engine, group: ObjectId) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(group, ButtonGroup::BUTTONS.index)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn is_exclusive(engine: &Engine, group: ObjectId) -> bool {
    engine.untrack(|| flag(engine, group, ButtonGroup::EXCLUSIVE.index))
}

fn is_checked(engine: &Engine, button: ObjectId) -> bool {
    engine.untrack(|| flag(engine, button, AbstractButton::CHECKED.index))
}

fn group_of(engine: &Engine, button: ObjectId) -> Option<ObjectId> {
    engine
        .with::<AbstractButton, _>(button, |b| b.state.group.get())
        .ok()
        .flatten()
}

fn add(engine: &Engine, group: ObjectId, button: ObjectId) -> Result<(), EngineError> {
    if !is_a::<AbstractButton>(engine, button) {
        return Err(EngineError::WrongType {
            ty: "ButtonGroup.buttons".into(),
            expected: "an AbstractButton".into(),
        });
    }
    match group_of(engine, button) {
        Some(g) if g == group => return Ok(()),
        Some(g) if engine.is_alive(g) => remove(engine, g, button)?,
        _ => {}
    }
    engine.with::<AbstractButton, _>(button, |b| b.state.group.set(Some(group)))?;
    engine.list_push(group, ButtonGroup::BUTTONS.index, button)?;
    if is_checked(engine, button) && is_exclusive(engine, group) {
        set_current(engine, group, Some(button))?;
    }
    recompute(engine, group)
}

fn remove(engine: &Engine, group: ObjectId, button: ObjectId) -> Result<(), EngineError> {
    if group_of(engine, button) != Some(group) {
        return Ok(());
    }
    let current = engine.with::<ButtonGroup, _>(group, |g| g.current)?;
    if current == Some(button) {
        // Leaving takes the check mark along (checked on qml6).
        set_current(engine, group, None)?;
    }
    engine.with::<AbstractButton, _>(button, |b| b.state.group.set(None))?;
    engine.list_remove(group, ButtonGroup::BUTTONS.index, button)?;
    recompute(engine, group)
}

/// Makes `button` the checked button of an exclusive group: it is
/// checked, and the one before it is not.
fn set_current(
    engine: &Engine,
    group: ObjectId,
    button: Option<ObjectId>,
) -> Result<(), EngineError> {
    let button = button.filter(|b| group_of(engine, *b) == Some(group));
    let old =
        engine.with_mut::<ButtonGroup, _>(group, |g| std::mem::replace(&mut g.current, button))?;
    let value = button.map_or(Value::Null, Value::Object);
    set_computed(engine, group, ButtonGroup::CHECKED_BUTTON.index, value)?;
    if old == button {
        return Ok(());
    }
    if let Some(o) = old.filter(|o| engine.is_alive(*o)) {
        set_checked(engine, o, false)?;
    }
    if let Some(b) = button {
        set_checked(engine, b, true)?;
    }
    Ok(())
}

/// A member's `checked` changed.
pub(crate) fn button_toggled(engine: &Engine, group: ObjectId, button: ObjectId, on: bool) {
    let r = (|| {
        let current = engine.with::<ButtonGroup, _>(group, |g| g.current)?;
        if on && is_exclusive(engine, group) {
            set_current(engine, group, Some(button))?;
        } else if !on && current == Some(button) {
            set_current(engine, group, None)?;
        }
        recompute(engine, group)
    })();
    check(engine, "ButtonGroup", r);
}

/// A member was clicked.
pub(crate) fn button_clicked(engine: &Engine, group: ObjectId, button: ObjectId) {
    let r = engine.emit_by_name(group, "clicked", &[Value::Object(button)]);
    check(engine, "ButtonGroup", r);
}

/// Brings `checkState` in step with the buttons.
fn recompute(engine: &Engine, group: ObjectId) -> Result<(), EngineError> {
    let all = members(engine, group);
    let on = all.iter().filter(|b| is_checked(engine, **b)).count();
    let s = if on == 0 {
        check_state::UNCHECKED
    } else if on == all.len() {
        check_state::CHECKED
    } else {
        check_state::PARTIALLY_CHECKED
    };
    engine.with_mut::<ButtonGroup, _>(group, |g| g.computed_state = s)?;
    set_computed(engine, group, ButtonGroup::CHECK_STATE.index, Value::Int(s))
}

/// `checkState` changed. If the group did not do it, the author did: in a
/// group that is not exclusive, "checked" and "unchecked" check and
/// uncheck every button; anything else is not something a group can be
/// told, and the property goes back.
fn state_assigned(engine: &Engine, group: ObjectId, s: i32) -> Result<(), EngineError> {
    let computed = engine.with::<ButtonGroup, _>(group, |g| g.computed_state)?;
    if s == computed {
        return Ok(());
    }
    let settable = s == check_state::UNCHECKED || s == check_state::CHECKED;
    if settable && !is_exclusive(engine, group) {
        for b in members(engine, group) {
            set_checked(engine, b, s == check_state::CHECKED)?;
        }
    }
    recompute(engine, group)
}

/// Follows `ButtonGroup.group` on buttons.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, owner, att| {
        if is_a::<ButtonGroupAttached>(e, att) {
            let p = ButtonGroupAttached::GROUP.index;
            let r = e.watch(att, p, move |e, v| {
                if !e.is_alive(owner) || !is_a::<AbstractButton>(e, owner) {
                    return;
                }
                let r = match v.as_object().filter(|g| is_a::<ButtonGroup>(e, *g)) {
                    Some(g) => add(e, g, owner),
                    None => match group_of(e, owner) {
                        Some(g) if e.is_alive(g) => remove(e, g, owner),
                        _ => Ok(()),
                    },
                };
                check(e, "ButtonGroup.group", r);
            });
            check(e, "ButtonGroup.group", r.map(|_| ()));
        }
        true
    });
}
