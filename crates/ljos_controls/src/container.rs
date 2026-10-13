//! `Container` (QtQuick.Templates): a control that keeps a list of items
//! and has a current one. `TabBar` and `SwipeView` are containers.
//!
//! The items are the rows of `contentModel`, an `ObjectModel` the
//! container keeps in step with them. A content item that is a view (the
//! Basic style's `ListView` on that model) places each item where its row
//! is; any other content item (a `Row`) gets the items as its children,
//! in order. A view's current row and the container's current index are
//! kept the same both ways.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-container.html.
//! How the current index moves when items come, go and move was read off
//! a running Qt 6.11.2; `features/controls/container.feature` pins it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    ConnectionId, Cx, Engine, EngineError, ObjectFlags, ObjectId, PropIndex, QmlList, QmlObject,
    Value, WriteKind, qml_impl,
};
use ljos_quick::{
    Item, ItemView, ObjectModel, Repeater, is_item, object_model_changed, object_model_rows,
    set_parent_item,
};

use crate::control::Control;
use crate::rows;
use crate::util::{bind_default, check, is_a, num, object, set_computed};

/// What a container remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct ContainerState {
    items: RefCell<Vec<ObjectId>>,
    /// The `ObjectModel` that `contentModel` reads.
    model: Cell<Option<ObjectId>>,
    /// The repeaters whose items are items of the container.
    repeaters: RefCell<Vec<ObjectId>>,
    /// The view in the content item whose current row is followed, and
    /// the connection that follows it.
    view: Cell<Option<(ObjectId, ConnectionId)>>,
    /// There was no item before the last change.
    was_empty: Cell<bool>,
    /// The rows of the model are being brought in step: what the view
    /// does to its current row meanwhile is not a swipe to follow.
    syncing: Cell<bool>,
}

/// `Container`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Container",
    extends = Control,
    default = "contentData"
)]
pub struct Container {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, list, append = container_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
    /// The items as a model: an `ObjectModel`, for a view to show.
    #[qml(notify, readonly)]
    pub content_model: Value,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub current_item: Option<ObjectId>,
    #[qml(notify, reset = CONTENT_ASKED)]
    pub content_width: f64,
    #[qml(notify, reset = CONTENT_ASKED)]
    pub content_height: f64,
    #[qml(skip)]
    pub(crate) state: Rc<ContainerState>,
}

impl Default for Container {
    fn default() -> Self {
        Self {
            control: Control::default(),
            content_data: QmlList::default(),
            content_children: QmlList::default(),
            content_model: Value::Null,
            count: 0,
            current_index: -1,
            current_item: None,
            content_width: 0.0,
            content_height: 0.0,
            state: Rc::default(),
        }
    }
}

/// `contentWidth = undefined`: back to what the content asks for.
const CONTENT_ASKED: ljos_engine::Reset = ljos_engine::Reset::Follow(content_asked);

fn content_asked(engine: &Engine, obj: ObjectId, size: PropIndex) -> Value {
    Value::Real(content_size(
        engine,
        obj,
        size == Container::CONTENT_WIDTH.index,
    ))
}

/// How big the content wants to be along one axis (tracked): what a tab
/// bar's buttons ask for together, or else what the content item asks.
fn content_size(engine: &Engine, obj: ObjectId, horizontal: bool) -> f64 {
    if let Some(size) = crate::tab_bar::content_size(engine, obj, horizontal) {
        return size;
    }
    let size = if horizontal {
        Item::IMPLICIT_WIDTH
    } else {
        Item::IMPLICIT_HEIGHT
    };
    object(engine, obj, Control::CONTENT_ITEM.index)
        .filter(|c| is_item(engine, *c))
        .map_or(0.0, |c| num(engine, c, size.index))
}

fn container_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let (e, me) = (cx.engine, cx.this);
    keep(e, me, child)?;
    let complete = e
        .flags(me)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if !complete {
        // Taken in when the container is complete.
        return Ok(());
    }
    take_in(e, me, child, None)?;
    changed(e, me, Change::Added)
}

fn index_arg(v: &Value) -> Option<i64> {
    match v {
        Value::Int(_) | Value::Real(_) => Some(v.to_number() as i64),
        _ => None,
    }
}

#[qml_impl]
impl Container {
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, index: Value) -> Value {
        index_arg(&index)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| items(cx.engine, cx.this).get(i).copied())
            .map_or(Value::Null, Value::Object)
    }

    #[qml(invokable, name = "addItem")]
    fn add_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        insert(cx.engine, cx.this, item, None)
    }

    #[qml(invokable, name = "insertItem")]
    fn insert_item(cx: &Cx, index: Value, item: Option<ObjectId>) -> Result<(), EngineError> {
        let at = index_arg(&index).map(|i| usize::try_from(i).unwrap_or(0));
        insert(cx.engine, cx.this, item, at)
    }

    #[qml(invokable, name = "moveItem")]
    fn move_item(cx: &Cx, from: Value, to: Value) -> Result<(), EngineError> {
        let (Some(from), Some(to)) = (index_arg(&from), index_arg(&to)) else {
            return Ok(());
        };
        move_rows(cx.engine, cx.this, from, to)
    }

    /// Takes the item out and destroys it.
    #[qml(invokable, name = "removeItem")]
    fn remove_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let Some(at) = item.and_then(|i| items(e, me).iter().position(|x| *x == i)) else {
            return Ok(());
        };
        if let Some(item) = take(e, me, at)? {
            e.destroy(item);
        }
        Ok(())
    }

    /// Takes the item at `index` out and returns it.
    #[qml(invokable, name = "takeItem")]
    fn take_item(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        let Some(at) = index_arg(&index).and_then(|i| usize::try_from(i).ok()) else {
            return Ok(Value::Null);
        };
        Ok(take(cx.engine, cx.this, at)?.map_or(Value::Null, Value::Object))
    }

    #[qml(invokable, name = "setCurrentIndex")]
    fn set_current_index(cx: &Cx, index: Value) -> Result<(), EngineError> {
        if let Some(i) = index_arg(&index) {
            set_current(cx.engine, cx.this, i as i32);
        }
        Ok(())
    }

    /// One item further, not past the last.
    #[qml(invokable, name = "incrementCurrentIndex")]
    fn increment_current_index(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let i = current_index(e, me);
        if i + 1 < count(e, me) {
            set_current(e, me, i + 1);
        }
    }

    /// One item back, not before the first.
    #[qml(invokable, name = "decrementCurrentIndex")]
    fn decrement_current_index(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let i = current_index(e, me);
        if i > 0 {
            set_current(e, me, i - 1);
        }
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this)
    }
}

pub(crate) fn state_of(engine: &Engine, me: ObjectId) -> Option<Rc<ContainerState>> {
    engine.with::<Container, _>(me, |c| c.state.clone()).ok()
}

/// The items, in order.
pub(crate) fn items(engine: &Engine, me: ObjectId) -> Vec<ObjectId> {
    state_of(engine, me).map_or_else(Vec::new, |s| {
        s.items
            .borrow()
            .iter()
            .copied()
            .filter(|i| engine.is_alive(*i))
            .collect()
    })
}

pub(crate) fn current_index(engine: &Engine, me: ObjectId) -> i32 {
    engine
        .untrack(|| engine.read(me, Container::CURRENT_INDEX.index))
        .to_number() as i32
}

fn count(engine: &Engine, me: ObjectId) -> i32 {
    items(engine, me).len() as i32
}

/// Makes `index` the current one as the container itself does (a click on
/// a tab, a swipe): a binding the author put on `currentIndex` stays.
pub(crate) fn set_current(engine: &Engine, me: ObjectId, index: i32) {
    let r = crate::popup::put(
        engine,
        me,
        Container::CURRENT_INDEX.index,
        Value::Int(index),
    );
    check(engine, "currentIndex", r);
}

/// Lists `obj` in `contentData` if it is not there.
fn keep(e: &Engine, me: ObjectId, obj: ObjectId) -> Result<(), EngineError> {
    let known = matches!(
        e.untrack(|| e.read(me, Container::CONTENT_DATA.index)),
        Value::List(l) if l.iter().any(|v| v.as_object() == Some(obj))
    );
    if !known {
        e.list_push(me, Container::CONTENT_DATA.index, obj)?;
    }
    Ok(())
}

fn init(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let model = e.create_native(ObjectModel::default(), Some(me))?;
    e.complete(model)?;
    if let Some(st) = state_of(e, me) {
        st.model.set(Some(model));
    }
    set_computed(e, me, Container::CONTENT_MODEL.index, Value::Object(model))?;
    let declared: Vec<ObjectId> = match e.untrack(|| e.read(me, Container::CONTENT_DATA.index)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    };
    for child in declared {
        if e.is_alive(child) {
            take_in(e, me, child, None)?;
        }
    }
    bind_default(e, me, Container::CONTENT_WIDTH.index, move |e| {
        Value::Real(content_size(e, me, true))
    })?;
    bind_default(e, me, Container::CONTENT_HEIGHT.index, move |e| {
        Value::Real(content_size(e, me, false))
    })?;
    e.watch(me, Control::CONTENT_ITEM.index, move |e, _| {
        check(e, "contentItem", rehome(e, me));
    })?;
    e.watch(me, Container::CURRENT_INDEX.index, move |e, _| {
        check(e, "currentIndex", current_changed(e, me));
    })?;
    rehome(e, me)?;
    changed(e, me, Change::Added)
}

/// Makes `obj` an item, the `at`-th or the last. A repeater's items are
/// items where the repeater stands; anything else is only kept.
fn take_in(e: &Engine, me: ObjectId, obj: ObjectId, at: Option<usize>) -> Result<(), EngineError> {
    if is_a::<Repeater>(e, obj) {
        return take_in_repeater(e, me, obj);
    }
    if !is_item(e, obj) || obj == me {
        return keep(e, me, obj);
    }
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    keep(e, me, obj)?;
    {
        let mut list = st.items.borrow_mut();
        if let Some(from) = list.iter().position(|i| *i == obj) {
            // Already an item: it moves.
            if let Some(to) = at {
                let item = list.remove(from);
                let to = to.min(list.len());
                list.insert(to, item);
            }
            return Ok(());
        }
        let at = at.map_or(list.len(), |i| i.min(list.len()));
        list.insert(at, obj);
    }
    if let Some(ci) = e.untrack(|| object(e, me, Control::CONTENT_ITEM.index)) {
        set_parent_item(e, obj, Some(rows::home(e, ci)))?;
    }
    Ok(())
}

/// A `Repeater` declared in a container is no item: what it makes are.
fn take_in_repeater(e: &Engine, me: ObjectId, repeater: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    keep(e, me, repeater)?;
    if st.repeaters.borrow().contains(&repeater) {
        return Ok(());
    }
    st.repeaters.borrow_mut().push(repeater);
    let item_of = |args: &[Value]| {
        let index = args.first().map(|v| v.to_number() as usize);
        args.get(1).and_then(Value::as_object).zip(index)
    };
    e.connect_by_name(repeater, "itemAdded", move |e, args| {
        if let Some((item, index)) = item_of(args).filter(|_| e.is_alive(me)) {
            let at = repeated_place(e, me, repeater, index);
            let r = take_in(e, me, item, Some(at)).and_then(|()| changed(e, me, Change::Added));
            check(e, "container", r);
        }
    })?;
    e.connect_by_name(repeater, "itemRemoved", move |e, args| {
        if let Some((item, _)) = item_of(args).filter(|_| e.is_alive(me)) {
            let at = items(e, me).iter().position(|i| *i == item);
            if let Some(at) = at {
                check(e, "container", take(e, me, at).map(|_| ()));
            }
        }
    })?;
    // The repeater makes its items beside itself, which is among the
    // container's items.
    if let Some(ci) = e.untrack(|| object(e, me, Control::CONTENT_ITEM.index)) {
        set_parent_item(e, repeater, Some(rows::home(e, ci)))?;
    }
    let made = e
        .with::<Repeater, _>(repeater, |r| r.delegates().map(|d| d.objects()))
        .ok()
        .flatten()
        .unwrap_or_default();
    for (index, item) in made.into_iter().enumerate() {
        let at = repeated_place(e, me, repeater, index);
        take_in(e, me, item, Some(at))?;
    }
    Ok(())
}

/// Where the `index`-th item of `repeater` goes among the items: after
/// the items of the same repeater before it.
fn repeated_place(e: &Engine, me: ObjectId, repeater: ObjectId, index: usize) -> usize {
    let made: Vec<ObjectId> = e
        .with::<Repeater, _>(repeater, |r| r.delegates().map(|d| d.objects()))
        .ok()
        .flatten()
        .unwrap_or_default();
    let list = items(e, me);
    let before: Vec<ObjectId> = made.into_iter().take(index).collect();
    match list.iter().rposition(|i| before.contains(i)) {
        Some(p) => p + 1,
        None => list.len(),
    }
}

fn insert(
    e: &Engine,
    me: ObjectId,
    item: Option<ObjectId>,
    at: Option<usize>,
) -> Result<(), EngineError> {
    let Some(item) = item.filter(|i| e.is_alive(*i) && is_item(e, *i)) else {
        return Ok(());
    };
    if let Some(from) = items(e, me).iter().position(|i| *i == item) {
        let to = at.unwrap_or(usize::MAX).min(count(e, me) as usize - 1);
        return move_rows(e, me, from as i64, to as i64);
    }
    take_in(e, me, item, at)?;
    changed(e, me, Change::Added)
}

fn move_rows(e: &Engine, me: ObjectId, from: i64, to: i64) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let n = count(e, me) as i64;
    if from < 0 || to < 0 || from >= n || to >= n || from == to {
        return Ok(());
    }
    {
        let mut list = st.items.borrow_mut();
        list.retain(|i| e.is_alive(*i));
        let item = list.remove(from as usize);
        list.insert(to as usize, item);
    }
    // The current item stays current wherever it goes.
    let cur = i64::from(current_index(e, me));
    let now = if cur == from {
        to
    } else if from < cur && to >= cur {
        cur - 1
    } else if from > cur && to <= cur {
        cur + 1
    } else {
        cur
    };
    changed(e, me, Change::Moved)?;
    if now != cur {
        set_current(e, me, now as i32);
    }
    Ok(())
}

/// Takes the `at`-th item out: it is no child of the content item any
/// more. None if there is no such item.
fn take(e: &Engine, me: ObjectId, at: usize) -> Result<Option<ObjectId>, EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(None);
    };
    let item = {
        let mut list = st.items.borrow_mut();
        list.retain(|i| e.is_alive(*i));
        if at >= list.len() {
            return Ok(None);
        }
        list.remove(at)
    };
    let left = count(e, me);
    let cur = current_index(e, me);
    e.list_remove(me, Container::CONTENT_DATA.index, item)?;
    // While the item is alive: a view lets go of its row.
    sync(e, me)?;
    if e.untrack(|| e.visual_parent(item)).is_some() {
        set_parent_item(e, item, None)?;
    }
    crate::tab_bar::left(e, me, item);
    crate::split_view::left(e, me, item);
    let at = at as i32;
    let now = if left == 0 {
        -1
    } else if at < cur {
        cur - 1
    } else if at == cur {
        (cur - 1).max(0)
    } else {
        cur
    };
    changed(e, me, Change::Removed)?;
    if now != cur {
        set_current(e, me, now);
    } else {
        current_changed(e, me)?;
    }
    Ok(Some(item))
}

/// What happened to the items.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Change {
    Added,
    Moved,
    Removed,
}

/// The items changed: their number or their order.
fn changed(e: &Engine, me: ObjectId, change: Change) -> Result<(), EngineError> {
    sync(e, me)?;
    if change == Change::Moved {
        restack(e, me)?;
    }
    let was_empty = state_of(e, me).is_some_and(|st| st.was_empty.get());
    // The first item of an empty container is current (qml6).
    if change == Change::Added && was_empty && current_index(e, me) < 0 && count(e, me) > 0 {
        set_current(e, me, 0);
        return Ok(());
    }
    current_changed(e, me)
}

/// Brings `count`, `contentChildren` and the rows of `contentModel` in step
/// with the items.
fn sync(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    st.items.borrow_mut().retain(|i| e.is_alive(*i));
    let want: Vec<ObjectId> = st.items.borrow().clone();
    let before = e.untrack(|| e.read(me, Container::COUNT.index)).to_number() as usize;
    st.was_empty.set(before == 0);
    if let Some(model) = st.model.get().filter(|m| e.is_alive(*m))
        && let Some(rows) = object_model_rows(e, model)
        && rows.objects() != want
    {
        // A view moves its current row on past rows that arrive at or
        // before it. Into an empty view, though, the first row arrives
        // where the current one is to be, and moves nothing (qml6); the
        // container says which item is current then. Without this, a view
        // whose index is already bound (to a tab bar's) counts the items.
        let view = st.view.get().map(|(v, _)| v).filter(|v| e.is_alive(*v));
        let held = view
            .filter(|_| before == 0)
            .map(|v| e.untrack(|| e.read(v, ItemView::CURRENT_INDEX.index)));
        st.syncing.set(held.is_some());
        let _done = Unset(&st.syncing);
        for (row, object) in rows.objects().into_iter().enumerate().rev() {
            if !want.contains(&object) {
                rows.remove(e, row, 1);
            }
        }
        for (row, object) in want.iter().enumerate() {
            match rows.index_of(*object) {
                Some(at) if at == row => {}
                Some(at) => {
                    rows.move_rows(e, at, row, 1);
                }
                None => rows.insert(e, row, *object),
            }
        }
        object_model_changed(e, model);
        if let Some((view, held)) = view.zip(held) {
            crate::popup::put(e, view, ItemView::CURRENT_INDEX.index, held)?;
        }
    }
    let list: Vec<Value> = want.iter().copied().map(Value::Object).collect();
    e.batch(|| {
        e.write_internal(
            me,
            Container::CONTENT_CHILDREN.index,
            Value::list(list),
            WriteKind::Initial,
        )?;
        set_computed(e, me, Container::COUNT.index, Value::Int(want.len() as i32))
    })?;
    crate::tab_bar::items_changed(e, me);
    crate::swipe_view::items_changed(e, me);
    crate::split_view::items_changed(e, me);
    Ok(())
}

/// Clears the flag however the scope is left.
struct Unset<'a>(&'a Cell<bool>);

impl Drop for Unset<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Under a content item that is no view the order of the children is the
/// order of the items (a `Row` places them so).
fn restack(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(ci) = e.untrack(|| object(e, me, Control::CONTENT_ITEM.index)) else {
        return Ok(());
    };
    if is_a::<ItemView>(e, ci) {
        return Ok(());
    }
    let home = rows::home(e, ci);
    for item in items(e, me) {
        set_parent_item(e, item, None)?;
        set_parent_item(e, item, Some(home))?;
    }
    Ok(())
}

/// The content item was set or replaced: the items go into it, and a view
/// in it is followed.
fn rehome(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    if let Some((_, conn)) = st.view.take() {
        e.disconnect(conn);
    }
    let Some(ci) = e
        .untrack(|| object(e, me, Control::CONTENT_ITEM.index))
        .filter(|c| is_item(e, *c))
    else {
        return Ok(());
    };
    let home = rows::home(e, ci);
    for repeater in st.repeaters.borrow().clone() {
        if e.is_alive(repeater) {
            set_parent_item(e, repeater, Some(home))?;
        }
    }
    for item in items(e, me) {
        if e.untrack(|| e.visual_parent(item)) != Some(home) {
            set_parent_item(e, item, Some(home))?;
        }
    }
    if is_a::<ItemView>(e, ci) {
        // A swipe or a key moves the view's current row; the container's
        // follows.
        let conn = e.connect_by_name(ci, "currentIndexChanged", move |e, _| {
            if !e.is_alive(me)
                || !e.is_alive(ci)
                || state_of(e, me).is_some_and(|st| st.syncing.get())
            {
                return;
            }
            let at = e
                .untrack(|| e.read(ci, ItemView::CURRENT_INDEX.index))
                .to_number() as i32;
            if at >= 0 && at != current_index(e, me) {
                set_current(e, me, at);
            }
        })?;
        st.view.set(Some((ci, conn)));
    }
    Ok(())
}

/// Registers the templates of this wave's navigation controls. `Container`
/// has no look of its own, so it is also what `QtQuick.Controls` names.
pub(crate) fn register(engine: &Engine) {
    use ljos_engine::{MetaRef, QmlType, Version};
    for meta in [
        Container::static_meta(),
        crate::tab_bar::TabBar::static_meta(),
        crate::tab_bar::TabButton::static_meta(),
        crate::swipe_view::SwipeView::static_meta(),
        crate::stack_view::StackView::static_meta(),
        crate::popup::Drawer::static_meta(),
        crate::split_view::SplitView::static_meta(),
        crate::split_view::SplitHandle::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    for uri in crate::CONTROLS {
        // Neither has a look of its own.
        for meta in [
            Container::static_meta(),
            crate::split_view::SplitHandle::static_meta(),
        ] {
            engine.register_type(uri, Version::new(2, 15), MetaRef::Static(meta));
        }
    }
    crate::split_view::register(engine);
}

/// `currentItem` follows `currentIndex` and the items.
fn current_changed(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let i = current_index(e, me);
    let item = usize::try_from(i)
        .ok()
        .and_then(|i| items(e, me).get(i).copied());
    set_computed(
        e,
        me,
        Container::CURRENT_ITEM.index,
        item.map_or(Value::Null, Value::Object),
    )?;
    crate::tab_bar::current_changed(e, me);
    crate::swipe_view::current_changed(e, me);
    Ok(())
}
