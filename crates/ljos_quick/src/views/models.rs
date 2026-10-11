//! `ObjectModel` and `DelegateModel` (QtQml.Models): the two models that
//! say themselves what shows their rows.
//!
//! The rows of an [`ObjectModel`] are items that exist already; a view
//! puts each where its row is and makes nothing. A [`DelegateModel`] is a
//! model together with the delegate of its rows; a view makes that
//! delegate for the rows it shows. Both are what a control hands the view
//! inside it: a menu its items, a combo box the rows of its popup.
//!
//! They live here and not with the engine's other models because a view is
//! what they are for; the engine's part is [`ObjectRows`] and
//! [`DelegateRows`], which any other reader of a model sees as rows too.

use std::rc::Rc;

use ljos_engine::{
    Cx, DelegateRows, Engine, EngineError, ItemModel, ModelChange, ObjectId, ObjectRows, QmlList,
    QmlObject, QtObject, Value, qml_impl,
};

use crate::item::is_a;

/// `ObjectModel` (QtQml.Models): a list of items that a view shows as
/// they are.
///
/// Not there: the attached `ObjectModel.index`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "ObjectModel",
    extends = QtObject,
    default = "children"
)]
pub struct ObjectModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(list, append = child_append)]
    pub children: QmlList,
    #[qml(skip)]
    pub(crate) rows: Rc<ObjectRows>,
}

fn child_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let rows = cx.with(|m: &ObjectModel| m.rows.clone())?;
    rows.insert(cx.engine, rows.len(), child);
    sync(cx.engine, cx.this, &rows)
}

/// Brings `children` and `count` in step with the rows.
fn sync(e: &Engine, model: ObjectId, rows: &ObjectRows) -> Result<(), EngineError> {
    let now: Vec<Value> = rows.objects().into_iter().map(Value::Object).collect();
    let count = now.len() as i32;
    e.batch(|| {
        e.write_internal(
            model,
            ObjectModel::CHILDREN.index,
            Value::list(now),
            ljos_engine::WriteKind::Initial,
        )?;
        e.set_internal(model, ObjectModel::COUNT, count)
    })
}

/// A row number or a count a method was given: a whole number that is
/// not negative.
fn whole(v: &Value) -> Option<usize> {
    let n = v.as_number()?;
    (n >= 0.0 && n.fract() == 0.0).then_some(n as usize)
}

/// A count a method may leave out: one when it does.
fn count_or_one(v: &Value) -> Option<usize> {
    match v {
        Value::Undefined => Some(1),
        v => whole(v),
    }
}

/// The rows of the `ObjectModel` `model`: what a native control that
/// owns one changes it through ([`object_model_changed`] after).
pub fn object_model_rows(e: &Engine, model: ObjectId) -> Option<Rc<ObjectRows>> {
    e.with(model, |m: &ObjectModel| m.rows.clone()).ok()
}

/// Brings what QML reads of the `ObjectModel` `model` (`count`,
/// `children`) in step after its rows were changed natively.
pub fn object_model_changed(e: &Engine, model: ObjectId) {
    if let Some(rows) = object_model_rows(e, model) {
        let _ = sync(e, model, &rows);
    }
}

#[qml_impl]
impl ObjectModel {
    /// For every reader of a model that is not a view (a `Repeater`).
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let rows = cx.with(|m: &ObjectModel| m.rows.clone())?;
        cx.engine.set_object_model(cx.this, Some(rows))
    }

    /// `get(index)`: the item of a row; `null` when there is no such row.
    #[qml(invokable)]
    fn get(cx: &Cx, index: Value) -> Value {
        let rows = object_model_rows(cx.engine, cx.this);
        whole(&index)
            .and_then(|i| rows?.get(i))
            .map_or(Value::Null, Value::Object)
    }

    /// `append(object)`: the item is the last row.
    #[qml(invokable)]
    fn append(cx: &Cx, object: Value) {
        let (Some(rows), Some(o)) = (object_model_rows(cx.engine, cx.this), object.as_object())
        else {
            return;
        };
        rows.insert(cx.engine, rows.len(), o);
        let _ = sync(cx.engine, cx.this, &rows);
    }

    /// `insert(index, object)`: the item is row `index`.
    #[qml(invokable)]
    fn insert(cx: &Cx, index: Value, object: Value) {
        let (Some(rows), Some(o)) = (object_model_rows(cx.engine, cx.this), object.as_object())
        else {
            return;
        };
        let Some(at) = whole(&index).filter(|i| *i <= rows.len()) else {
            return;
        };
        rows.insert(cx.engine, at, o);
        let _ = sync(cx.engine, cx.this, &rows);
    }

    /// `move(from, to, n = 1)`: `n` rows from `from` start at `to`.
    #[qml(invokable, name = "move")]
    fn move_rows(cx: &Cx, from: Value, to: Value, count: Value) {
        let Some(rows) = object_model_rows(cx.engine, cx.this) else {
            return;
        };
        let (Some(from), Some(to), Some(n)) = (whole(&from), whole(&to), count_or_one(&count))
        else {
            return;
        };
        if rows.move_rows(cx.engine, from, to, n) {
            let _ = sync(cx.engine, cx.this, &rows);
        }
    }

    /// `remove(index, n = 1)`: `n` rows from `index` go. The items stay
    /// alive.
    #[qml(invokable)]
    fn remove(cx: &Cx, index: Value, count: Value) {
        let Some(rows) = object_model_rows(cx.engine, cx.this) else {
            return;
        };
        let (Some(first), Some(n)) = (whole(&index), count_or_one(&count)) else {
            return;
        };
        if first.checked_add(n).is_some_and(|end| end <= rows.len()) {
            rows.remove(cx.engine, first, n);
            let _ = sync(cx.engine, cx.this, &rows);
        }
    }

    /// `clear()`: no rows. The items stay alive.
    #[qml(invokable)]
    fn clear(cx: &Cx) {
        if let Some(rows) = object_model_rows(cx.engine, cx.this) {
            rows.clear(cx.engine);
            let _ = sync(cx.engine, cx.this, &rows);
        }
    }
}

/// `DelegateModel` (QtQml.Models): a model and the delegate of its rows.
///
/// Not there: groups (`items`, `persistedItems`, `groups`, `filterOnGroup`
/// and the attached properties), `parts`, `rootIndex` and the methods
/// that walk a tree model.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "DelegateModel",
    extends = QtObject,
    default = "delegate"
)]
pub struct DelegateModel {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(skip)]
    pub(crate) rows: Rc<DelegateRows>,
    /// A native control gives the rows their model and delegate itself;
    /// the two properties are then only what QML reads back.
    #[qml(skip)]
    pub(crate) driven: bool,
}

/// The rows of the `DelegateModel` `model`, for the native control that
/// owns it: from this call on the control says what the model and the
/// delegate are ([`DelegateRows::set_model`] and
/// [`DelegateRows::set_component`]), and the object's two properties are
/// no longer read.
pub fn drive_delegate_model(e: &Engine, model: ObjectId) -> Option<Rc<DelegateRows>> {
    e.with_mut(model, |m: &mut DelegateModel| {
        m.driven = true;
        m.rows.clone()
    })
    .ok()
}

#[qml_impl]
impl DelegateModel {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let rows = cx.with(|m: &DelegateModel| m.rows.clone())?;
        e.set_object_model(me, Some(rows.clone()))?;
        let counted = Rc::downgrade(&rows);
        // The model outlives no one: the object holds it, and it holds
        // this listener.
        rows.subscribe(Rc::new(move |e: &Engine, c: &ModelChange| {
            let about = matches!(
                c,
                ModelChange::RowsAboutToBeInserted { .. }
                    | ModelChange::RowsAboutToBeRemoved { .. }
            );
            if let (false, Some(rows)) = (about, counted.upgrade())
                && e.is_alive(me)
            {
                let _ = e.set_internal(me, DelegateModel::COUNT, rows.row_count() as i32);
            }
        }));
        let _ = e.set_internal(me, DelegateModel::COUNT, rows.row_count() as i32);
        // What the rows were last read from: a new delegate alone is no
        // new model.
        let mut shown: Option<Value> = None;
        e.effect_in(me, move |e| {
            let model = e.get(me, DelegateModel::MODEL).unwrap_or_default();
            let delegate = e.get(me, DelegateModel::DELEGATE).ok().flatten();
            if e.with(me, |m: &DelegateModel| m.driven).unwrap_or(true) {
                return;
            }
            let same = shown.as_ref().is_some_and(|s| s.change_equals(&model));
            shown = Some(model.clone());
            e.untrack(|| {
                rows.set_component(e, delegate);
                if !same {
                    rows.set_model(e, Some(e.model_from_value(&model, Some(me))));
                }
            });
        })?;
        Ok(())
    }
}

/// The model a view shows when `model:` holds one of the two types here.
/// A view asks before it asks the engine: the answer does not depend on
/// which of the two objects was completed first.
pub(crate) fn model_of(e: &Engine, v: &Value) -> Option<Rc<dyn ItemModel>> {
    let o = v.as_object().filter(|o| e.is_alive(*o))?;
    if is_a::<ObjectModel>(e, o) {
        let rows = e.with(o, |m: &ObjectModel| m.rows.clone()).ok()?;
        return Some(rows);
    }
    if is_a::<DelegateModel>(e, o) {
        let rows = e.with(o, |m: &DelegateModel| m.rows.clone()).ok()?;
        return Some(rows);
    }
    None
}

pub(crate) fn register(engine: &Engine) {
    use ljos_engine::{MetaRef, QmlType, Version};
    for meta in [ObjectModel::static_meta(), DelegateModel::static_meta()] {
        engine.register_type("QtQml.Models", Version::new(2, 0), MetaRef::Static(meta));
    }
}
