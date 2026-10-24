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

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, DelegateRows, Engine, EngineError, ItemModel, ModelChange, ObjectId, ObjectRows,
    QmlList, QmlObject, QtObject, Value, qml_impl,
};

use super::delegate_groups::{self, GroupState, count_of, index_of, strings_of};
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
    #[qml(list, append = child_append, insert = child_insert)]
    pub children: QmlList,
    #[qml(skip)]
    pub(crate) rows: Rc<ObjectRows>,
}

fn child_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let rows = cx.with(|m: &ObjectModel| m.rows.clone())?;
    rows.insert(cx.engine, rows.len(), child);
    sync(cx.engine, cx.this, &rows)
}

/// [`child_append`], with `child` right before `anchor`.
fn child_insert(cx: &Cx, child: ObjectId, anchor: ObjectId) -> Result<(), EngineError> {
    let rows = cx.with(|m: &ObjectModel| m.rows.clone())?;
    let at = rows.objects().iter().position(|o| *o == anchor);
    rows.insert(cx.engine, at.unwrap_or(rows.len()), child);
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

/// The model `obj` is to a view that names it, when it is one: an
/// `ObjectModel`'s or a `DelegateModel`'s rows, or what the engine knows
/// ([`Engine::object_model`]). For an inspector that lists the models.
pub fn model_of_object(e: &Engine, obj: ObjectId) -> Option<Rc<dyn ItemModel>> {
    model_of(e, &Value::Object(obj)).or_else(|| e.object_model(obj))
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

/// The DelegateModel a group belongs to, and the entries it holds its
/// members among.
type GroupLink = Rc<RefCell<Option<(ObjectId, Rc<GroupState>)>>>;

/// `DelegateModelGroup` (QtQml.Models): a named set of a
/// `DelegateModel`'s items, which items are put in and taken out of and
/// moved about in independently of the model under it.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "DelegateModelGroup",
    extends = QtObject
)]
pub struct DelegateModelGroup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify)]
    pub name: Atom,
    #[qml(notify)]
    pub include_by_default: bool,
    /// The DelegateModel this group is a group of, and the entries it
    /// holds its members among: set when that model takes the group.
    #[qml(skip)]
    pub(crate) link: GroupLink,
}

fn linked_group(cx: &Cx) -> Option<(Rc<GroupState>, String)> {
    let link = cx.with(|g: &DelegateModelGroup| g.link.clone()).ok()?;
    let (_, state) = link.borrow().clone()?;
    let name = state.name_of(cx.this).unwrap_or_else(|| {
        cx.engine
            .read(cx.this, DelegateModelGroup::NAME.index)
            .to_js_string()
            .to_string()
    });
    Some((state, name))
}

#[qml_impl]
impl DelegateModelGroup {
    #[qml(signal)]
    fn changed(removed: Value, inserted: Value);

    #[qml(signal, name = "defaultIncludeChanged")]
    fn default_include_changed();

    /// `addGroups(index, count, groups)`: the items also belong to
    /// `groups`.
    #[qml(invokable, name = "addGroups")]
    fn add_groups(cx: &Cx, index: Value, count: Value, groups: Value) {
        if let (Some((state, name)), Some(i), Some(n)) =
            (linked_group(cx), index_of(&index), count_of(&count))
        {
            state.add_groups(cx.engine, &name, i, n, strings_of(cx.engine, &groups));
        }
    }

    /// `removeGroups(index, count, groups)`: the items no longer belong
    /// to `groups`.
    #[qml(invokable, name = "removeGroups")]
    fn remove_groups(cx: &Cx, index: Value, count: Value, groups: Value) {
        if let (Some((state, name)), Some(i), Some(n)) =
            (linked_group(cx), index_of(&index), count_of(&count))
        {
            state.remove_groups(cx.engine, &name, i, n, strings_of(cx.engine, &groups));
        }
    }

    /// `setGroups(index, count, groups)`: the items belong to `groups`
    /// and to no other group.
    #[qml(invokable, name = "setGroups")]
    fn set_groups(cx: &Cx, index: Value, count: Value, groups: Value) {
        if let (Some((state, name)), Some(i), Some(n)) =
            (linked_group(cx), index_of(&index), count_of(&count))
        {
            state.set_groups(cx.engine, &name, i, n, strings_of(cx.engine, &groups));
        }
    }

    /// `remove(index, count)`: the items leave this group.
    #[qml(invokable)]
    fn remove(cx: &Cx, index: Value, count: Value) {
        if let (Some((state, name)), Some(i), Some(n)) =
            (linked_group(cx), index_of(&index), count_of(&count))
        {
            state.remove_from_group(cx.engine, &name, i, n);
        }
    }

    /// `move(from, to, count)`: items of this group move to a new place
    /// in it, which is a new place among the DelegateModel's items as a
    /// whole. The model under it is not told.
    #[qml(invokable, name = "move")]
    fn move_rows(cx: &Cx, from: Value, to: Value, count: Value) {
        if let (Some((state, name)), Some(f), Some(t), Some(n)) = (
            linked_group(cx),
            index_of(&from),
            index_of(&to),
            count_of(&count),
        ) {
            state.move_in_group(cx.engine, &name, f, t, n);
        }
    }

    /// `insert(data, groups?)` and `insert(index, data, groups?)`: a new
    /// item of the data, at the index in this group when one is given and
    /// after its last member when there is none, in the groups named or
    /// in this one.
    #[qml(invokable)]
    fn insert(cx: &Cx, a: Value, b: Value, c: Value) {
        let Some((state, name)) = linked_group(cx) else {
            return;
        };
        let (index, data, groups) = match index_of(&a) {
            Some(i) => (Some(i), &b, &c),
            None => (None, &a, &b),
        };
        let data = cx
            .engine
            .js_object_entries(data)
            .into_iter()
            .collect::<Vec<_>>();
        if data.is_empty() {
            return;
        }
        let groups = match groups {
            Value::Undefined => None,
            g => Some(strings_of(cx.engine, g)),
        };
        state.insert_entry(cx.engine, &name, index, data, groups);
    }
}

/// What `DelegateModel.` attaches to each delegate it makes: the
/// DelegateModel, the groups its item is in, and its place in the two
/// groups every DelegateModel has.
///
/// Not there: an `in<Group>` and a `<group>Index` for every other group,
/// which Qt names after the groups of the one DelegateModel (see
/// `delegate_groups`).
#[derive(Debug, Default, QmlObject)]
#[qml(name = "DelegateModelAttached")]
pub struct DelegateModelAttached {
    #[qml(notify, readonly)]
    pub model: Option<ObjectId>,
    #[qml(notify)]
    pub groups: Value,
    #[qml(notify, readonly)]
    pub is_unresolved: bool,
    #[qml(notify)]
    pub in_items: bool,
    #[qml(notify)]
    pub in_persisted_items: bool,
    #[qml(notify, readonly)]
    pub items_index: i32,
    #[qml(notify, readonly)]
    pub persisted_items_index: i32,
}

/// `DelegateModel` (QtQml.Models): a model and the delegate of its rows,
/// with the groups of its items.
///
/// Kept and read back, with no effect: `rootIndex` (there is no model
/// whose rows have children), and `parts`, which is nothing until there is
/// a `Package` for a delegate to be.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQml.Models",
    name = "DelegateModel",
    extends = QtObject,
    attached = DelegateModelAttached,
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
    #[qml(notify, readonly)]
    pub items: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub persisted_items: Option<ObjectId>,
    #[qml(list)]
    pub groups: QmlList,
    #[qml(notify, reset)]
    pub filter_on_group: Atom,
    #[qml(notify, readonly)]
    pub parts: Option<ObjectId>,
    #[qml(notify)]
    pub root_index: Value,
    #[qml(skip)]
    pub(crate) rows: Rc<DelegateRows>,
    #[qml(skip)]
    pub(crate) groups_state: Rc<GroupState>,
    /// A native control gives the rows their model and delegate itself;
    /// the two properties are then only what QML reads back.
    #[qml(skip)]
    pub(crate) driven: bool,
}

impl Default for DelegateModel {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            model: Value::Undefined,
            delegate: None,
            count: 0,
            items: None,
            persisted_items: None,
            groups: QmlList::new(),
            filter_on_group: Atom::from("items"),
            parts: None,
            root_index: Value::Undefined,
            rows: Rc::default(),
            groups_state: Rc::new(GroupState::default()),
            driven: false,
        }
    }
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

/// One of the two groups every DelegateModel has, made for it.
fn builtin_group(
    e: &Engine,
    dm: ObjectId,
    state: &Rc<GroupState>,
    name: &str,
    include_by_default: bool,
) -> Option<ObjectId> {
    let o = e
        .create_native_incomplete(
            DelegateModelGroup {
                object: QtObject::default(),
                count: 0,
                name: Atom::from(name),
                include_by_default,
                link: Rc::new(RefCell::new(Some((dm, state.clone())))),
            },
            Some(dm),
        )
        .ok()?;
    let _ = e.complete(o);
    Some(o)
}

fn list_objects(v: &Value) -> Vec<ObjectId> {
    match v {
        Value::List(items) => items.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

#[qml_impl]
impl DelegateModel {
    /// The signal `filterOnGroup` is told with, under the name Qt gives
    /// it; `filterOnGroupChanged` is there from the property besides.
    #[qml(signal, name = "filterGroupChanged")]
    fn filter_group_changed();

    #[qml(signal, name = "defaultGroupsChanged")]
    fn default_groups_changed();

    /// `modelIndex(index)`: kept with `rootIndex`; there is no index of
    /// a tree model to give.
    #[qml(invokable, name = "modelIndex")]
    fn model_index(_cx: &Cx, _index: Value) -> Value {
        Value::Undefined
    }

    #[qml(invokable, name = "parentModelIndex")]
    fn parent_model_index(cx: &Cx) -> Value {
        let _ = cx;
        Value::Undefined
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let rows = cx.with(|m: &DelegateModel| m.rows.clone())?;
        let state = cx.with(|m: &DelegateModel| m.groups_state.clone())?;
        state.bind_dm(me);
        // The two groups every DelegateModel has: all its items, and the
        // ones a view lets go of that are kept. Their counts follow the
        // entries from here on.
        let builtins: [Option<ObjectId>; 2] = [
            builtin_group(e, me, &state, "items", true),
            builtin_group(e, me, &state, "persistedItems", false),
        ];
        let _ = e.set_internal(me, DelegateModel::ITEMS, builtins[0]);
        let _ = e.set_internal(me, DelegateModel::PERSISTED_ITEMS, builtins[1]);
        // Delegates are told their entry, so that `DelegateModel.` of a
        // delegate can be answered whenever it is asked for. A native
        // control driving these rows puts its own word in place of this
        // one (ComboBox does), and its delegates are then not told.
        {
            let watched = state.clone();
            rows.on_allocated(Some(Rc::new(move |e: &Engine, row, object| {
                if let Some(key) = watched.entry_key_at_filtered(row) {
                    delegate_groups::register_delegate(e, me, &watched, object, key);
                }
            })));
            let watched = state.clone();
            rows.on_delegates(
                Some(Rc::new(move |e: &Engine, row, object| {
                    if let Some(key) = watched.entry_key_at_filtered(row) {
                        delegate_groups::register_delegate(e, me, &watched, object, key);
                    }
                })),
                Some(Rc::new(move |e: &Engine, object| {
                    delegate_groups::unregister_delegate(e, object);
                })),
            );
        }
        e.set_object_model(me, Some(rows.clone()))?;
        // The rows read the model through the groups from now on.
        rows.set_model(e, Some(state.clone() as Rc<dyn ItemModel>));
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
        let mut shown: Option<(Value, u64)> = None;
        let mut shown_filter = String::new();
        e.effect_in(me, move |e| {
            let model = e.get(me, DelegateModel::MODEL).unwrap_or_default();
            let delegate = e.get(me, DelegateModel::DELEGATE).ok().flatten();
            let given = list_objects(&e.read(me, DelegateModel::GROUPS.index));
            let mut customs: Vec<(ObjectId, String, bool)> = Vec::new();
            for g in given {
                if builtins.contains(&Some(g)) || !is_a::<DelegateModelGroup>(e, g) {
                    continue;
                }
                let name = e
                    .read(g, DelegateModelGroup::NAME.index)
                    .to_js_string()
                    .to_string();
                let include = e
                    .read(g, DelegateModelGroup::INCLUDE_BY_DEFAULT.index)
                    .to_boolean();
                customs.push((g, name, include));
            }
            let filter = e
                .read(me, DelegateModel::FILTER_ON_GROUP.index)
                .to_js_string()
                .to_string();
            if e.with(me, |m: &DelegateModel| m.driven).unwrap_or(true) {
                return;
            }
            let generation = e.track_model(&model);
            let same = shown
                .as_ref()
                .is_some_and(|(s, g)| s.change_equals(&model) && *g == generation);
            shown = Some((model.clone(), generation));
            if shown_filter != filter {
                shown_filter.clone_from(&filter);
                let _ = e.emit_by_name(me, "filterGroupChanged", &[]);
            }
            e.untrack(|| {
                // Every group the model holds is one of its entries'
                // groups, two of its own first, and `groups` reads all
                // of them back in that order.
                let mut defs: Vec<(Option<ObjectId>, String, bool)> = vec![
                    (builtins[0], "items".to_owned(), true),
                    (builtins[1], "persistedItems".to_owned(), false),
                ];
                for (g, _, _) in &customs {
                    let link = Rc::new(RefCell::new(Some((me, state.clone()))));
                    let _ = e.with_mut(*g, |m: &mut DelegateModelGroup| {
                        m.link = link;
                    });
                }
                for (g, name, include) in &customs {
                    // A group the author says takes new items by default
                    // is taken at its word when its name is read back.
                    let include = *include
                        || e.read(*g, DelegateModelGroup::INCLUDE_BY_DEFAULT.index)
                            .to_boolean();
                    defs.push((Some(*g), name.clone(), include));
                }
                state.apply_defs_and_filter(e, defs, filter);
                let all: Vec<Value> = builtins
                    .iter()
                    .flatten()
                    .chain(customs.iter().map(|(g, _, _)| g))
                    .map(|g| Value::Object(*g))
                    .collect();
                let now = list_objects(&e.read(me, DelegateModel::GROUPS.index));
                let want: Vec<ObjectId> = all.iter().filter_map(Value::as_object).collect();
                if now != want {
                    let _ = e.write_internal(
                        me,
                        DelegateModel::GROUPS.index,
                        Value::list(all),
                        ljos_engine::WriteKind::Initial,
                    );
                }
                rows.set_component(e, delegate);
                if !same {
                    state.set_underlying(e, Some(e.model_from_value(&model, Some(me))));
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
    for meta in [
        ObjectModel::static_meta(),
        DelegateModel::static_meta(),
        DelegateModelGroup::static_meta(),
    ] {
        engine.register_type("QtQml.Models", Version::new(2, 0), MetaRef::Static(meta));
    }
    engine.on_attached_created_of::<DelegateModelAttached>(|e, item, att| {
        delegate_groups::attached_created(e, item, att);
        true
    });
}
