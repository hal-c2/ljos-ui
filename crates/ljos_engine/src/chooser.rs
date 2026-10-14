//! `DelegateChooser` and `DelegateChoice` (`QtQml.Models`, `Qt.labs.qmlmodels`):
//! a delegate that picks a `Component` for each row from an ordered list of
//! `DelegateChoice`s.

use std::rc::Rc;

use crate::atom::Atom;
use crate::builtins::QtObject;
use crate::cx::Cx;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::list::QmlList;
use crate::model::ItemModel;
use crate::object::QmlType;
use crate::qtqml::Component;
use crate::value::Value;
use crate::{Provenance, QmlObject, qml_impl};

/// `DelegateChoice`: one branch of a [`DelegateChooser`].
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "DelegateChoice",
    extends = QtObject,
    default = "delegate"
)]
pub struct DelegateChoice {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub role_value: Value,
    #[qml(notify)]
    pub row: i32,
    #[qml(notify)]
    pub index: i32,
    #[qml(notify)]
    pub column: i32,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
}

impl Default for DelegateChoice {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            role_value: Value::Undefined,
            row: -1,
            index: -1,
            column: -1,
            delegate: None,
        }
    }
}

#[qml_impl]
impl DelegateChoice {
    #[qml(signal)]
    fn changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let row_set = cx
            .engine
            .provenance(me, DelegateChoice::ROW.index)
            .is_some_and(|p| p != Provenance::Default);
        let idx_set = cx
            .engine
            .provenance(me, DelegateChoice::INDEX.index)
            .is_some_and(|p| p != Provenance::Default);
        if idx_set && !row_set {
            let idx = cx.get(DelegateChoice::INDEX)?;
            cx.set(DelegateChoice::ROW, idx)?;
        } else if row_set && !idx_set {
            let row = cx.get(DelegateChoice::ROW)?;
            cx.set(DelegateChoice::INDEX, row)?;
        }
        cx.engine
            .watch(me, DelegateChoice::ROW.index, move |e, v| {
                if let Some(r) = v.as_number() {
                    let r = r as i32;
                    let cur = e.untrack(|| e.get(me, DelegateChoice::INDEX)).unwrap_or(-1);
                    if cur != r {
                        let _ = e.set(me, DelegateChoice::INDEX, r);
                    }
                }
                if let Some(s) = DelegateChoice::static_meta().signal_index("changed") {
                    let _ = e.emit(me, s, &[]);
                }
            })?;
        cx.engine
            .watch(me, DelegateChoice::INDEX.index, move |e, v| {
                if let Some(i) = v.as_number() {
                    let i = i as i32;
                    let cur = e.untrack(|| e.get(me, DelegateChoice::ROW)).unwrap_or(-1);
                    if cur != i {
                        let _ = e.set(me, DelegateChoice::ROW, i);
                    }
                }
                if let Some(s) = DelegateChoice::static_meta().signal_index("changed") {
                    let _ = e.emit(me, s, &[]);
                }
            })?;
        for prop in [
            DelegateChoice::ROLE_VALUE.index,
            DelegateChoice::COLUMN.index,
            DelegateChoice::DELEGATE.index,
        ] {
            cx.engine.watch(me, prop, move |e, _| {
                if let Some(s) = DelegateChoice::static_meta().signal_index("changed") {
                    let _ = e.emit(me, s, &[]);
                }
            })?;
        }
        Ok(())
    }
}

/// `DelegateChooser`: picks a `Component` for each row from [`Self::choices`].
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "DelegateChooser",
    extends = Component,
    default = "choices"
)]
pub struct DelegateChooser {
    #[qml(base)]
    pub component: Component,
    #[qml(notify)]
    pub role: Atom,
    #[qml(list, readonly)]
    pub choices: QmlList,
}

/// Whether `obj` is a [`DelegateChooser`].
pub(crate) fn is_chooser(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .untrack(|| engine.with::<DelegateChooser, _>(obj, |_| ()))
        .is_ok()
}

/// Whether a `DataChanged` notification on `roles` touches the role `chooser`
/// inspects (and therefore remakes the affected rows' delegates).
pub(crate) fn role_affected(engine: &Engine, chooser: ObjectId, roles: &[Atom]) -> bool {
    if roles.is_empty() {
        return true;
    }
    let Ok(role) =
        engine.untrack(|| engine.with::<DelegateChooser, _>(chooser, |c| c.role.clone()))
    else {
        return false;
    };
    role.is_empty() || roles.contains(&role)
}

/// If `component` is a [`DelegateChooser`], returns the `Component` chosen
/// for row `index` (or `None` when no [`DelegateChoice`] matches or the
/// matching choice has no delegate). Otherwise returns `Some(component)`.
pub(crate) fn resolve_delegate(
    engine: &Engine,
    component: ObjectId,
    model: &Rc<dyn ItemModel>,
    index: usize,
) -> Option<ObjectId> {
    if !is_chooser(engine, component) {
        return Some(component);
    }
    choose(engine, component, model.as_ref(), index, 0)
}

fn choose(
    engine: &Engine,
    chooser: ObjectId,
    model: &dyn ItemModel,
    index: usize,
    column: usize,
) -> Option<ObjectId> {
    let Ok((role, choices)) = engine.untrack(|| {
        engine.with::<DelegateChooser, _>(chooser, |c| (c.role.clone(), c.choices.clone()))
    }) else {
        return None;
    };
    let mut row_val: Option<Value> = None;
    for choice_id in choices.objects() {
        let Ok((expected, row, idx, col, delegate)) = engine.untrack(|| {
            engine.with::<DelegateChoice, _>(choice_id, |c| {
                (c.role_value.clone(), c.row, c.index, c.column, c.delegate)
            })
        }) else {
            continue;
        };
        let choice_row = if row >= 0 { row } else { idx };
        if choice_row >= 0 && choice_row as usize != index {
            continue;
        }
        if col >= 0 && col as usize != column {
            continue;
        }
        if !matches!(expected, Value::Undefined) {
            let actual = row_val.get_or_insert_with(|| {
                engine.untrack(|| row_role_value(engine, model, index, &role))
            });
            if !actual.strict_equals(&expected) {
                continue;
            }
        }
        return delegate;
    }
    None
}

fn row_role_value(engine: &Engine, model: &dyn ItemModel, index: usize, role: &str) -> Value {
    if !role.is_empty() {
        let v = model.data(engine, index, role);
        if !matches!(v, Value::Undefined) {
            return v;
        }
        if role == "modelData"
            && let Some(md) = model.model_data(engine, index)
        {
            return md;
        }
        if let Some(Value::Object(o)) = model.model_data(engine, index) {
            return engine.read_by_name(o, role);
        }
        return Value::Undefined;
    }
    if let Some(md) = model.model_data(engine, index) {
        return md;
    }
    let roles = model.role_names();
    if roles.len() == 1 {
        return model.data(engine, index, &roles[0]);
    }
    Value::Undefined
}
