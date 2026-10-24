//! `ActionGroup` (QtQuick.Controls): actions that belong together, of
//! which, in an exclusive group, one is checked.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-actiongroup.html;
//! pinned by `features/controls/action-group.feature`, which records what
//! `qml6` (Qt 6.11.2) does where the documentation is silent: unchecking
//! the checked action leaves it the group's `checkedAction`, and
//! triggering it again leaves it checked.
//!
//! A disabled group makes its actions read `enabled: false` without
//! taking their own value: as for an item under a disabled parent, an
//! override layer covers it, an assignment while covered is the action's
//! own new value, and the own value is what lets a script `trigger()` the
//! action (both checked on qml6).

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_engine::{
    Cx, Engine, EngineError, ObjectId, OverrideContent, OverrideId, Provenance, QmlList, QmlObject,
    QtObject, RestoreMode, Value, WriteKind, qml_impl,
};

use crate::action::Action;
use crate::util::{check, flag, is_a, object, set_computed};

/// `ActionGroup`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ActionGroup",
    extends = QtObject,
    attached = ActionGroupAttached,
    default = "actions"
)]
pub struct ActionGroup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, list, append = group_append)]
    pub actions: QmlList,
    #[qml(notify)]
    pub checked_action: Option<ObjectId>,
    #[qml(notify)]
    pub exclusive: bool,
    #[qml(notify)]
    pub enabled: bool,
    /// The checked action as the group last settled it, so that a change
    /// of `checkedAction` knows which action to uncheck.
    #[qml(skip)]
    current: Option<ObjectId>,
}

impl Default for ActionGroup {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            actions: QmlList::default(),
            checked_action: None,
            exclusive: true,
            enabled: true,
            current: None,
        }
    }
}

/// What `ActionGroup.group: someGroup` attaches to an action.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "ActionGroupAttached")]
pub struct ActionGroupAttached {
    #[qml(notify)]
    pub group: Option<ObjectId>,
}

/// Which group each action is in, for the action to ask.
#[derive(Default)]
struct Membership {
    group_of: RefCell<HashMap<ObjectId, ObjectId>>,
    /// The layer that makes `enabled` read false, on each action whose
    /// own value is true and whose group is disabled.
    covered: RefCell<HashMap<ObjectId, OverrideId>>,
}

fn group_of(engine: &Engine, action: ObjectId) -> Option<ObjectId> {
    let m = engine.extension::<Membership>();
    let g = m.group_of.borrow().get(&action).copied();
    g.filter(|g| engine.is_alive(*g))
}

fn group_append(cx: &Cx, action: ObjectId) -> Result<(), EngineError> {
    add(cx.engine, cx.this, action)
}

#[qml_impl]
impl ActionGroup {
    /// An action of the group was triggered.
    #[qml(signal)]
    fn triggered(action: Option<ObjectId>);

    #[qml(invokable, name = "addAction")]
    fn add_action(cx: &Cx, action: Option<ObjectId>) -> Result<(), EngineError> {
        match action {
            Some(a) => add(cx.engine, cx.this, a),
            None => Ok(()),
        }
    }

    #[qml(invokable, name = "removeAction")]
    fn remove_action(cx: &Cx, action: Option<ObjectId>) -> Result<(), EngineError> {
        match action {
            Some(a) => remove(cx.engine, cx.this, a),
            None => Ok(()),
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, group) = (cx.engine, cx.this);
        let wanted = e.untrack(|| object(e, group, ActionGroup::CHECKED_ACTION.index));
        if wanted.is_some() {
            set_current(e, group, wanted)?;
        }
        e.watch(group, ActionGroup::CHECKED_ACTION.index, move |e, v| {
            let a = v.as_object().filter(|a| e.is_alive(*a));
            check(e, "checkedAction", set_current(e, group, a));
        })?;
        e.watch(group, ActionGroup::ENABLED.index, move |e, _| {
            check(e, "enabled", group_enabled(e, group));
        })?;
        if !e.untrack(|| flag(e, group, ActionGroup::ENABLED.index)) {
            group_enabled(e, group)?;
        }
        Ok(())
    }
}

fn members(engine: &Engine, group: ObjectId) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(group, ActionGroup::ACTIONS.index)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

fn is_exclusive(engine: &Engine, group: ObjectId) -> bool {
    engine.untrack(|| flag(engine, group, ActionGroup::EXCLUSIVE.index))
}

fn is_checked(engine: &Engine, action: ObjectId) -> bool {
    engine.untrack(|| flag(engine, action, Action::CHECKED.index))
}

/// Writes a property of an action the way an assignment would, except
/// that a binding on it stays.
fn put(
    engine: &Engine,
    obj: ObjectId,
    p: ljos_engine::PropIndex,
    on: bool,
) -> Result<(), EngineError> {
    if engine.is_bound(obj, p) {
        engine.write_derived(obj, p, Value::Bool(on))
    } else {
        engine.write(obj, p, Value::Bool(on), WriteKind::Imperative)
    }
}

fn add(engine: &Engine, group: ObjectId, action: ObjectId) -> Result<(), EngineError> {
    if !is_a::<Action>(engine, action) {
        return Err(EngineError::WrongType {
            ty: "ActionGroup.actions".into(),
            expected: "an Action".into(),
        });
    }
    match group_of(engine, action) {
        Some(g) if g == group => return Ok(()),
        Some(g) => remove(engine, g, action)?,
        None => {}
    }
    let first_time = engine
        .extension::<Membership>()
        .group_of
        .borrow_mut()
        .insert(action, group)
        .is_none();
    engine.list_push(group, ActionGroup::ACTIONS.index, action)?;
    if first_time {
        // Watched once for the life of the action; a group it has left
        // is not told.
        engine.watch(action, Action::CHECKED.index, move |e, v| {
            if v.to_boolean()
                && let Some(g) = group_of(e, action)
                && is_exclusive(e, g)
            {
                check(e, "ActionGroup", set_current(e, g, Some(action)));
            }
        })?;
        // `true` assigned while the group is disabled is covered again.
        engine.watch(action, Action::ENABLED.index, move |e, _| {
            check(e, "enabled", cover(e, action));
        })?;
    }
    cover(engine, action)?;
    if is_checked(engine, action) && is_exclusive(engine, group) {
        set_current(engine, group, Some(action))?;
    }
    Ok(())
}

fn remove(engine: &Engine, group: ObjectId, action: ObjectId) -> Result<(), EngineError> {
    if group_of(engine, action) != Some(group) {
        return Ok(());
    }
    let current = engine.with::<ActionGroup, _>(group, |g| g.current)?;
    if current == Some(action) {
        // Leaving takes the check mark along (checked on qml6).
        set_current(engine, group, None)?;
    }
    engine
        .extension::<Membership>()
        .group_of
        .borrow_mut()
        .remove(&action);
    engine.list_remove(group, ActionGroup::ACTIONS.index, action)?;
    cover(engine, action)
}

/// Makes `action` the checked action of the group: it is checked, and the
/// one before it is not.
fn set_current(
    engine: &Engine,
    group: ObjectId,
    action: Option<ObjectId>,
) -> Result<(), EngineError> {
    let action = action.filter(|a| group_of(engine, *a) == Some(group));
    let old =
        engine.with_mut::<ActionGroup, _>(group, |g| std::mem::replace(&mut g.current, action))?;
    let value = action.map_or(Value::Null, Value::Object);
    set_computed(engine, group, ActionGroup::CHECKED_ACTION.index, value)?;
    if old == action {
        return Ok(());
    }
    if let Some(o) = old.filter(|o| engine.is_alive(*o)) {
        put(engine, o, Action::CHECKED.index, false)?;
    }
    if let Some(a) = action {
        put(engine, a, Action::CHECKED.index, true)?;
    }
    Ok(())
}

fn group_on(engine: &Engine, action: ObjectId) -> bool {
    group_of(engine, action)
        .is_none_or(|g| engine.untrack(|| flag(engine, g, ActionGroup::ENABLED.index)))
}

fn covering(engine: &Engine, action: ObjectId) -> Option<OverrideId> {
    let m = engine.extension::<Membership>();
    m.covered.borrow().get(&action).copied()
}

/// Makes `enabled` of `action` read its own value and its group's: a
/// layer that reads false covers an own `true` while the group is
/// disabled.
fn cover(engine: &Engine, action: ObjectId) -> Result<(), EngineError> {
    let p = Action::ENABLED.index;
    let m = engine.extension::<Membership>();
    if !engine.is_alive(action) {
        m.covered.borrow_mut().remove(&action);
        return Ok(());
    }
    let read = engine.untrack(|| flag(engine, action, p));
    let layer = covering(engine, action);
    match (group_on(engine, action), layer) {
        (true, None) => Ok(()),
        (true, Some(id)) => {
            m.covered.borrow_mut().remove(&action);
            // Something assigned through the layer is the action's own.
            let mode = if engine.provenance(action, p) == Some(Provenance::Assigned) {
                RestoreMode::None
            } else {
                RestoreMode::BindingOrValue
            };
            engine.remove_override(action, p, id, mode)
        }
        // An own `false` reads as it is; it is covered once it turns true.
        (false, _) if !read => Ok(()),
        (false, old) => {
            m.covered.borrow_mut().remove(&action);
            if let Some(id) = old {
                // `true` was assigned through the layer: it stays as the
                // own value, under a new layer.
                engine.remove_override(action, p, id, RestoreMode::None)?;
            }
            let id = engine.push_override(
                action,
                p,
                OverrideContent::Binding(Box::new(|_| Value::Bool(false))),
            )?;
            m.covered.borrow_mut().insert(action, id);
            Ok(())
        }
    }
}

fn group_enabled(engine: &Engine, group: ObjectId) -> Result<(), EngineError> {
    for action in members(engine, group) {
        cover(engine, action)?;
    }
    Ok(())
}

/// The `enabled` the action has of its own, under what its group makes
/// it read: what lets a script trigger it.
pub(crate) fn own_enabled(engine: &Engine, action: ObjectId) -> bool {
    let read = engine.untrack(|| flag(engine, action, Action::ENABLED.index));
    match covering(engine, action) {
        // Only an own `true` is covered, until something is assigned
        // through the layer.
        Some(_) => {
            read || engine.provenance(action, Action::ENABLED.index) != Some(Provenance::Assigned)
        }
        None => read,
    }
}

/// Whether triggering `action` leaves it as it is: it is the checked
/// action of an exclusive group, which always has one checked.
pub(crate) fn keeps_checked(engine: &Engine, action: ObjectId) -> bool {
    group_of(engine, action).is_some_and(|g| is_exclusive(engine, g)) && is_checked(engine, action)
}

/// `action` was triggered: its group says so.
pub(crate) fn triggered(engine: &Engine, action: ObjectId) -> Result<(), EngineError> {
    match group_of(engine, action) {
        Some(g) => engine.emit_by_name(g, "triggered", &[Value::Object(action)]),
        None => Ok(()),
    }
}

/// Follows `ActionGroup.group` on actions.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created_of::<ActionGroupAttached>(|e, owner, att| {
        let p = ActionGroupAttached::GROUP.index;
        let r = e.watch(att, p, move |e, v| {
            if !e.is_alive(owner) || !is_a::<Action>(e, owner) {
                return;
            }
            let r = match v.as_object().filter(|g| is_a::<ActionGroup>(e, *g)) {
                Some(g) => add(e, g, owner),
                None => match group_of(e, owner) {
                    Some(g) => remove(e, g, owner),
                    None => Ok(()),
                },
            };
            check(e, "ActionGroup.group", r);
        });
        check(e, "ActionGroup.group", r.map(|_| ()));
        true
    });
}
