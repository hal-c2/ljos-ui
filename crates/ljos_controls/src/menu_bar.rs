//! `MenuBar` and `MenuBarItem` (QtQuick.Templates).
//!
//! A menu bar is a row of buttons, one for each of its menus. What is
//! declared in it becomes one of them: a `Menu` as an item made from the
//! bar's `delegate`, a `MenuBarItem` as it is. One item at most is the
//! bar's current one, the one that shows `highlighted`: the item under
//! the pointer, or the item whose menu is open.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-menubar.html and
//! `-menubaritem.html`. When a menu opens and closes, which item is
//! highlighted, where the keyboard is and what the arrow keys do were
//! read off a running Qt 6.11.2; `features/controls/menu-bar.feature`
//! and `menu-bar-item.feature` pin them.
//!
//! Qt's bar is a `Container`. This one has the items and the menus and
//! none of the rest of a container (`currentIndex`, `moveItem`,
//! `contentModel`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_a11y::Role;
use ljos_engine::{
    ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectFlags, ObjectId, QmlList, QmlObject,
    QmlType, Value, WriteKind, qml_impl,
};
use ljos_quick::focus::{FocusReason, force_active_focus, register_focus_scope};
use ljos_quick::{Item, child_items, is_item, set_parent_item};

use crate::button::{AbstractButton, held_by_pointer};
use crate::containers::{Host, content_size, host_complete};
use crate::control::{Control, focus_policy, takes_input};
use crate::menu::Menu;
use crate::popup::{self, Popup, close_policy};
use crate::util::{bind_default, check, flag, is_a, is_unset, num, object, set_computed};

/// What a menu of a bar closes on: a press on its own item is the item's
/// to answer (measured on Qt 6.11.2, where a menu on its own has 17).
const BAR_MENU_POLICY: i32 = close_policy::ON_ESCAPE
    | close_policy::ON_PRESS_OUTSIDE_PARENT
    | close_policy::ON_RELEASE_OUTSIDE_PARENT;

// ---------------------------------------------------------- MenuBarItem

/// `MenuBarItem`: the button of one menu of a bar.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "MenuBarItem",
    extends = AbstractButton
)]
pub struct MenuBarItem {
    #[qml(base)]
    pub button: AbstractButton,
    /// The bar the item is in.
    #[qml(notify, readonly)]
    pub menu_bar: Option<ObjectId>,
    /// The menu the item opens.
    #[qml(notify)]
    pub menu: Option<ObjectId>,
    /// The item is the bar's current one.
    #[qml(notify)]
    pub highlighted: bool,
    /// The press that is down, or was last, came from the pointer.
    #[qml(skip)]
    by_pointer: Rc<Cell<bool>>,
}

impl Default for MenuBarItem {
    fn default() -> Self {
        let mut button = AbstractButton::default();
        // The keyboard reaches a bar through its menus, not by Tab.
        button.control.focus_policy = focus_policy::NONE;
        Self {
            button,
            menu_bar: None,
            menu: None,
            highlighted: false,
            by_pointer: Rc::default(),
        }
    }
}

#[qml_impl]
impl MenuBarItem {
    /// The user chose the item: its menu is to open, or to close.
    #[qml(signal)]
    fn triggered();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, item) = (cx.engine, cx.this);
        let by_pointer = e.with::<MenuBarItem, _>(item, |i| i.by_pointer.clone())?;
        let say = move |e: &Engine| {
            if e.is_alive(item) {
                check(e, "triggered", e.emit_by_name(item, "triggered", &[]));
            }
        };
        // The pointer chooses when it goes down, a key when the click is
        // done (the order of the two signals on Qt 6.11.2).
        let flag = by_pointer.clone();
        e.connect_by_name(item, "pressed", move |e, _| {
            let pointer = held_by_pointer(e, item);
            flag.set(pointer);
            if pointer {
                say(e);
            }
        })?;
        e.connect_by_name(item, "clicked", move |e, _| {
            if !by_pointer.get() {
                say(e);
            }
        })?;
        Ok(())
    }
}

// -------------------------------------------------------------- MenuBar

/// An item of a bar and what the bar put on it.
#[derive(Debug)]
struct Slot {
    item: ObjectId,
    /// The bar made the item, for a menu.
    made: bool,
    /// What the bar listens to on the item.
    conns: Vec<ConnectionId>,
    /// The menu the bar took for the item's, and what it listens to on it.
    menu: Option<(ObjectId, Vec<ConnectionId>)>,
}

/// What a bar remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct BarState {
    slots: RefCell<Vec<Slot>>,
    /// What was declared in the bar and is no item of it.
    others: RefCell<Vec<ObjectId>>,
    /// The bar is making an item from its delegate, which appends it.
    making: Cell<bool>,
    /// The highlighted item.
    current: Cell<Option<ObjectId>>,
    /// The bar is closing a menu itself, to open another or because its
    /// item was chosen again: the item stays what it was.
    switching: Cell<bool>,
}

/// `MenuBar`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "MenuBar",
    extends = Control,
    default = "contentData"
)]
pub struct MenuBar {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, list, append = bar_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
    /// How many items the bar has.
    #[qml(notify, readonly)]
    pub count: i32,
    /// What the item of a menu is made from.
    #[qml(notify)]
    pub delegate: Option<ObjectId>,
    /// The menus of the items, in the order of the bar.
    #[qml(notify, list, append = menus_append)]
    pub menus: QmlList,
    #[qml(notify, reset = CONTENT_ASKED)]
    pub content_width: f64,
    #[qml(notify, reset = CONTENT_ASKED)]
    pub content_height: f64,
    #[qml(skip)]
    pub(crate) state: Rc<BarState>,
}

/// `contentWidth = undefined`: the bar has what its content asks for again.
const CONTENT_ASKED: ljos_engine::Reset = ljos_engine::Reset::Follow(content_asked);

fn content_asked(engine: &Engine, bar: ObjectId, size: ljos_engine::PropIndex) -> Value {
    let asked = if size == MenuBar::CONTENT_WIDTH.index {
        Item::IMPLICIT_WIDTH
    } else {
        Item::IMPLICIT_HEIGHT
    };
    Value::Real(content_size(engine, bar, BAR, asked.index))
}

impl Default for MenuBar {
    fn default() -> Self {
        Self {
            control: Control {
                // `Qt.ClickFocus`.
                focus_policy: focus_policy::CLICK,
                ..Control::default()
            },
            content_data: QmlList::default(),
            content_children: QmlList::default(),
            count: 0,
            delegate: None,
            menus: QmlList::default(),
            content_width: 0.0,
            content_height: 0.0,
            state: Rc::default(),
        }
    }
}

const BAR: Host = Host {
    content_item: Control::CONTENT_ITEM.index,
    content_data: MenuBar::CONTENT_DATA.index,
    content_children: MenuBar::CONTENT_CHILDREN.index,
};

fn is_complete(e: &Engine, bar: ObjectId) -> bool {
    e.flags(bar)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED))
}

fn bar_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let (e, bar) = (cx.engine, cx.this);
    if !is_complete(e, bar) {
        // Taken in when the bar is complete, when its delegate is known.
        return e.list_push(bar, BAR.content_data, child);
    }
    if state_of(e, bar).is_some_and(|s| s.making.get()) {
        // The bar takes in what it makes itself.
        return Ok(());
    }
    take_in(e, bar, child, None)?;
    changed(e, bar)
}

fn menus_append(cx: &Cx, menu: ObjectId) -> Result<(), EngineError> {
    let (e, bar) = (cx.engine, cx.this);
    if !is_complete(e, bar) {
        return e.list_push(bar, MenuBar::MENUS.index, menu);
    }
    if is_a::<Menu>(e, menu) {
        take_in(e, bar, menu, None)?;
        changed(e, bar)?;
    }
    Ok(())
}

fn index_arg(v: &Value) -> Option<usize> {
    match v {
        Value::Int(_) | Value::Real(_) => usize::try_from(v.to_number() as i64).ok(),
        _ => None,
    }
}

fn or_null(o: Option<ObjectId>) -> Value {
    o.map_or(Value::Null, Value::Object)
}

#[qml_impl]
impl MenuBar {
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, index: Value) -> Value {
        or_null(index_arg(&index).and_then(|i| items(cx.engine, cx.this).get(i).copied()))
    }

    #[qml(invokable, name = "addItem")]
    fn add_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, item, None, false)
    }

    #[qml(invokable, name = "insertItem")]
    fn insert_item(cx: &Cx, index: Value, item: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, item, index_arg(&index), false)
    }

    /// Takes the item out of the bar and destroys it.
    #[qml(invokable, name = "removeItem")]
    fn remove_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        let (e, bar) = (cx.engine, cx.this);
        if let Some(item) = item
            && leave(e, bar, item)?.is_some()
        {
            e.destroy(item);
            changed(e, bar)?;
        }
        Ok(())
    }

    /// Takes the item at `index` out of the bar and returns it.
    #[qml(invokable, name = "takeItem")]
    fn take_item(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        let (e, bar) = (cx.engine, cx.this);
        let Some(item) = index_arg(&index).and_then(|i| items(e, bar).get(i).copied()) else {
            return Ok(Value::Null);
        };
        leave(e, bar, item)?;
        changed(e, bar)?;
        Ok(Value::Object(item))
    }

    #[qml(invokable, name = "menuAt")]
    fn menu_at(cx: &Cx, index: Value) -> Value {
        let e = cx.engine;
        or_null(
            index_arg(&index)
                .and_then(|i| items(e, cx.this).get(i).copied())
                .and_then(|item| menu_of(e, item)),
        )
    }

    #[qml(invokable, name = "addMenu")]
    fn add_menu(cx: &Cx, menu: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, menu, None, true)
    }

    #[qml(invokable, name = "insertMenu")]
    fn insert_menu(cx: &Cx, index: Value, menu: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, menu, index_arg(&index), true)
    }

    /// Takes the menu out of the bar and destroys it, with its item.
    #[qml(invokable, name = "removeMenu")]
    fn remove_menu(cx: &Cx, menu: Option<ObjectId>) -> Result<(), EngineError> {
        let (e, bar) = (cx.engine, cx.this);
        let Some(menu) = menu else {
            return Ok(());
        };
        let Some(item) = items(e, bar)
            .into_iter()
            .find(|i| menu_of(e, *i) == Some(menu))
        else {
            return Ok(());
        };
        if leave(e, bar, item)?.is_some_and(|slot| slot.made) {
            e.destroy(item);
        }
        e.destroy(menu);
        changed(e, bar)
    }

    /// Takes the menu at `index` out of the bar and returns it. The item
    /// the bar made for it is destroyed.
    #[qml(invokable, name = "takeMenu")]
    fn take_menu(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        let (e, bar) = (cx.engine, cx.this);
        let Some(item) = index_arg(&index).and_then(|i| items(e, bar).get(i).copied()) else {
            return Ok(Value::Null);
        };
        let Some(menu) = menu_of(e, item) else {
            return Ok(Value::Null);
        };
        if leave(e, bar, item)?.is_some_and(|slot| slot.made) {
            e.destroy(item);
        }
        changed(e, bar)?;
        Ok(Value::Object(menu))
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_bar(cx.engine, cx.this)
    }
}

fn state_of(engine: &Engine, bar: ObjectId) -> Option<Rc<BarState>> {
    engine.with::<MenuBar, _>(bar, |b| b.state.clone()).ok()
}

/// The items of a bar, in order.
fn items(engine: &Engine, bar: ObjectId) -> Vec<ObjectId> {
    state_of(engine, bar).map_or_else(Vec::new, |s| {
        s.slots
            .borrow()
            .iter()
            .map(|slot| slot.item)
            .filter(|i| engine.is_alive(*i))
            .collect()
    })
}

/// The menu `item` opens.
fn menu_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    if !is_a::<MenuBarItem>(engine, item) {
        return None;
    }
    engine
        .untrack(|| object(engine, item, MenuBarItem::MENU.index))
        .filter(|m| is_a::<Menu>(engine, *m))
}

fn is_open(engine: &Engine, menu: ObjectId) -> bool {
    engine.untrack(|| flag(engine, menu, Popup::VISIBLE.index))
}

fn init_bar(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    let listed = |prop| match e.untrack(|| e.read(bar, prop)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let declared = listed(BAR.content_data);
    let menus = listed(MenuBar::MENUS.index);
    // Both lists are written anew from the items once those are known.
    e.list_clear(bar, BAR.content_data)?;
    host_complete(e, bar, BAR)?;
    for child in declared.into_iter().chain(menus) {
        if e.is_alive(child) {
            take_in(e, bar, child, None)?;
        }
    }
    changed(e, bar)?;

    bind_default(e, bar, MenuBar::CONTENT_WIDTH.index, move |e| {
        Value::Real(content_size(e, bar, BAR, Item::IMPLICIT_WIDTH.index))
    })?;
    bind_default(e, bar, MenuBar::CONTENT_HEIGHT.index, move |e| {
        Value::Real(content_size(e, bar, BAR, Item::IMPLICIT_HEIGHT.index))
    })?;
    e.bind_initial(bar, Control::IMPLICIT_CONTENT_WIDTH.index, move |e| {
        Value::Real(num(e, bar, MenuBar::CONTENT_WIDTH.index))
    })?;
    e.bind_initial(bar, Control::IMPLICIT_CONTENT_HEIGHT.index, move |e| {
        Value::Real(num(e, bar, MenuBar::CONTENT_HEIGHT.index))
    })?;
    crate::a11y::defaults(e, bar, Role::MenuBar, None)
}

/// Makes `obj` an item of the bar, the `at`-th or the last: a
/// `MenuBarItem` as it is, a menu as an item made for it. Any other item
/// has no place in a bar (as on Qt 6.11.2), and what is no item is only
/// kept.
fn take_in(e: &Engine, bar: ObjectId, obj: ObjectId, at: Option<usize>) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    if is_a::<MenuBarItem>(e, obj) {
        return join(e, bar, obj, at, false);
    }
    if is_a::<Menu>(e, obj) {
        let known = items(e, bar)
            .into_iter()
            .find(|i| menu_of(e, *i) == Some(obj));
        if let Some(item) = known {
            return join(e, bar, item, at, false);
        }
        let item = make_item(e, bar, &st)?;
        if is_a::<MenuBarItem>(e, item) {
            e.write(
                item,
                MenuBarItem::MENU.index,
                Value::Object(obj),
                WriteKind::Imperative,
            )?;
        }
        e.bind(item, AbstractButton::TEXT.index, move |e| {
            if e.is_alive(obj) {
                e.read(obj, Menu::TITLE.index)
            } else {
                e.untrack(|| e.read(item, AbstractButton::TEXT.index))
            }
        })?;
        return join(e, bar, item, at, true);
    }
    if !is_item(e, obj) && !st.others.borrow().contains(&obj) {
        st.others.borrow_mut().push(obj);
    }
    Ok(())
}

/// An item from the bar's `delegate`, or a plain `MenuBarItem` if that
/// makes none.
fn make_item(e: &Engine, bar: ObjectId, st: &BarState) -> Result<ObjectId, EngineError> {
    if let Some(d) = e.untrack(|| object(e, bar, MenuBar::DELEGATE.index)) {
        st.making.set(true);
        let made = e.component_create(d, Some(bar), Vec::new());
        st.making.set(false);
        match made {
            Ok(item) if is_item(e, item) => return Ok(item),
            Ok(other) => {
                e.destroy(other);
            }
            Err(err) => check(e, "delegate", Err(err)),
        }
    }
    let item = e.create_native(MenuBarItem::default(), Some(bar))?;
    e.complete(item)?;
    Ok(item)
}

/// Takes `item` into the bar as its `at`-th item, or its last. An item
/// that is in the bar already moves there.
fn join(
    e: &Engine,
    bar: ObjectId,
    item: ObjectId,
    at: Option<usize>,
    made: bool,
) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    {
        let mut slots = st.slots.borrow_mut();
        if let Some(from) = slots.iter().position(|s| s.item == item) {
            if let Some(to) = at {
                let slot = slots.remove(from);
                let to = to.min(slots.len());
                slots.insert(to, slot);
            }
            return Ok(());
        }
    }
    let mut slot = Slot {
        item,
        made,
        conns: Vec::new(),
        menu: None,
    };
    if is_a::<MenuBarItem>(e, item) {
        set_computed(e, item, MenuBarItem::MENU_BAR.index, Value::Object(bar))?;
        slot.conns = vec![
            e.connect_by_name(item, "triggered", move |e, _| {
                check(e, "menu bar", item_triggered(e, bar, item));
            })?,
            e.connect_by_name(item, "hoveredChanged", move |e, _| {
                check(e, "menu bar", item_hovered(e, bar, item));
            })?,
            e.connect_by_name(item, "menuChanged", move |e, _| {
                check(e, "menu bar", wire(e, bar, item));
            })?,
        ];
    }
    {
        let mut slots = st.slots.borrow_mut();
        let at = at.map_or(slots.len(), |i| i.min(slots.len()));
        slots.insert(at, slot);
    }
    wire(e, bar, item)
}

/// Makes the menu `item` has now the one the bar opens for it.
fn wire(e: &Engine, bar: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    let menu = menu_of(e, item);
    let old = {
        let mut slots = st.slots.borrow_mut();
        let Some(slot) = slots.iter_mut().find(|s| s.item == item) else {
            return Ok(());
        };
        if slot.menu.as_ref().map(|(m, _)| *m) == menu {
            return Ok(());
        }
        slot.menu.take()
    };
    if let Some((_, conns)) = old {
        for c in conns {
            e.disconnect(c);
        }
    }
    if let Some(menu) = menu {
        // A menu of a bar is placed from its item.
        e.write(
            menu,
            Popup::PARENT.index,
            Value::Object(item),
            WriteKind::Imperative,
        )?;
        let policy = Popup::CLOSE_POLICY.index;
        if is_unset(e, menu, policy) {
            popup::put(e, menu, policy, Value::Int(BAR_MENU_POLICY))?;
        }
        let conns = vec![
            e.connect_by_name(menu, "aboutToShow", move |e, _| {
                if e.is_alive(item) {
                    set_current(e, bar, Some(item));
                }
            })?,
            e.connect_by_name(menu, "closed", move |e, _| menu_closed(e, bar, item))?,
        ];
        if let Some(slot) = st.slots.borrow_mut().iter_mut().find(|s| s.item == item) {
            slot.menu = Some((menu, conns));
        }
    }
    sync_menus(e, bar)
}

/// Takes `item` out of the bar: it is no child of it any more. None if
/// it was not in the bar.
fn leave(e: &Engine, bar: ObjectId, item: ObjectId) -> Result<Option<Slot>, EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(None);
    };
    let mut slot = {
        let mut slots = st.slots.borrow_mut();
        match slots.iter().position(|s| s.item == item) {
            Some(i) => slots.remove(i),
            None => return Ok(None),
        }
    };
    for c in slot.conns.drain(..) {
        e.disconnect(c);
    }
    if let Some((menu, conns)) = slot.menu.take() {
        for c in conns {
            e.disconnect(c);
        }
        if e.is_alive(menu) && is_open(e, menu) {
            popup::close(e, menu)?;
        }
    }
    if st.current.get() == Some(item) {
        st.current.set(None);
    }
    if !e.is_alive(item) {
        return Ok(Some(slot));
    }
    set_parent_item(e, item, None)?;
    if is_a::<MenuBarItem>(e, item) {
        set_computed(e, item, MenuBarItem::MENU_BAR.index, Value::Null)?;
        popup::put(e, item, MenuBarItem::HIGHLIGHTED.index, Value::Bool(false))?;
    }
    Ok(Some(slot))
}

/// `addItem`, `addMenu` and their `insert` forms.
fn add(
    e: &Engine,
    bar: ObjectId,
    obj: Option<ObjectId>,
    at: Option<usize>,
    menu: bool,
) -> Result<(), EngineError> {
    let Some(obj) = obj.filter(|o| e.is_alive(*o)) else {
        return Ok(());
    };
    let fits = if menu {
        is_a::<Menu>(e, obj)
    } else {
        is_a::<MenuBarItem>(e, obj)
    };
    if !fits {
        return Ok(());
    }
    take_in(e, bar, obj, at)?;
    changed(e, bar)
}

/// Writes `menus` anew from the items.
fn sync_menus(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    let menus: Vec<Value> = items(e, bar)
        .into_iter()
        .filter_map(|i| menu_of(e, i))
        .map(Value::Object)
        .collect();
    let now = e.untrack(|| e.read(bar, MenuBar::MENUS.index));
    if matches!(&now, Value::List(l) if **l == menus) {
        return Ok(());
    }
    e.write_internal(
        bar,
        MenuBar::MENUS.index,
        Value::list(menus),
        WriteKind::Imperative,
    )
}

/// The items of the bar changed: their number, the lists that name them
/// and the order they stand in.
fn changed(e: &Engine, bar: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    st.slots.borrow_mut().retain(|s| e.is_alive(s.item));
    st.others.borrow_mut().retain(|o| e.is_alive(*o));
    let items = items(e, bar);

    // The row shows its children in their order, and a child only ever
    // goes to the end: when the order is another, all go again.
    if let Some(ci) = e.untrack(|| object(e, bar, BAR.content_item)) {
        let there: Vec<ObjectId> = e
            .untrack(|| child_items(e, ci))
            .into_iter()
            .filter(|c| items.contains(c))
            .collect();
        if there != items {
            e.batch(|| {
                for item in &there {
                    set_parent_item(e, *item, None)?;
                }
                for item in &items {
                    set_parent_item(e, *item, Some(ci))?;
                }
                Ok::<(), EngineError>(())
            })?;
        }
    }

    let children: Vec<Value> = items.iter().copied().map(Value::Object).collect();
    let mut data = children.clone();
    data.extend(st.others.borrow().iter().copied().map(Value::Object));
    for (prop, list) in [(BAR.content_children, children), (BAR.content_data, data)] {
        let now = e.untrack(|| e.read(bar, prop));
        if !matches!(&now, Value::List(l) if **l == list) {
            e.write_internal(bar, prop, Value::list(list), WriteKind::Imperative)?;
        }
    }
    set_computed(e, bar, MenuBar::COUNT.index, Value::Int(items.len() as i32))?;
    sync_menus(e, bar)
}

// ------------------------------------------------------ the current item

/// Makes `item` the highlighted one, or none.
fn set_current(e: &Engine, bar: ObjectId, item: Option<ObjectId>) {
    let Some(st) = state_of(e, bar) else {
        return;
    };
    st.current.set(item);
    for i in items(e, bar) {
        if is_a::<MenuBarItem>(e, i) {
            let on = Value::Bool(Some(i) == item);
            let r = popup::put(e, i, MenuBarItem::HIGHLIGHTED.index, on);
            check(e, "highlighted", r);
        }
    }
}

/// The menu of the bar that is open, and its item.
fn open_one(e: &Engine, bar: ObjectId) -> Option<(ObjectId, ObjectId)> {
    items(e, bar)
        .into_iter()
        .find_map(|i| menu_of(e, i).filter(|m| is_open(e, *m)).map(|m| (i, m)))
}

/// Closes a menu as the bar's own doing: its item stays what it is.
fn close_quietly(e: &Engine, st: &BarState, menu: ObjectId) -> Result<(), EngineError> {
    let before = st.switching.replace(true);
    let closed = popup::close(e, menu);
    st.switching.set(before);
    closed
}

/// Opens the menu of `item` under it.
fn open_under(e: &Engine, item: ObjectId, menu: ObjectId) -> Result<(), EngineError> {
    let below = e.untrack(|| num(e, item, Item::HEIGHT.index));
    popup::move_to(e, menu, 0.0, below)?;
    popup::open(e, menu)
}

/// A menu of the bar closed. When the user turned away from it (Escape,
/// a press beside it, an item chosen) its item is no longer the current
/// one, wherever the pointer is.
fn menu_closed(e: &Engine, bar: ObjectId, item: ObjectId) {
    let Some(st) = state_of(e, bar) else {
        return;
    };
    if !st.switching.get() && st.current.get() == Some(item) {
        set_current(e, bar, None);
    }
}

/// The item was chosen: its menu opens, or closes if it is open.
fn item_triggered(e: &Engine, bar: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    set_current(e, bar, Some(item));
    let Some(menu) = menu_of(e, item) else {
        return Ok(());
    };
    if is_open(e, menu) {
        close_quietly(e, &st, menu)?;
        // The item that sent its menu away has the keyboard.
        if e.is_alive(item) {
            force_active_focus(e, item, FocusReason::Mouse);
        }
        return Ok(());
    }
    if let Some((_, other)) = open_one(e, bar) {
        close_quietly(e, &st, other)?;
    }
    open_under(e, item, menu)
}

/// The pointer came over an item, or left it. Over an item while another
/// item's menu is open, the open menu becomes this item's.
fn item_hovered(e: &Engine, bar: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, bar) else {
        return Ok(());
    };
    let over = e.untrack(|| flag(e, item, Control::HOVERED.index));
    let menu = menu_of(e, item);
    if !over {
        // The item of an open menu stays highlighted.
        if st.current.get() == Some(item) && !menu.is_some_and(|m| is_open(e, m)) {
            set_current(e, bar, None);
        }
        return Ok(());
    }
    if !takes_input(e, item) {
        return Ok(());
    }
    match open_one(e, bar) {
        Some((other, open)) if other != item => {
            close_quietly(e, &st, open)?;
            set_current(e, bar, Some(item));
            match menu {
                Some(menu) => open_under(e, item, menu),
                None => Ok(()),
            }
        }
        _ => {
            set_current(e, bar, Some(item));
            Ok(())
        }
    }
}

// ----------------------------------------------------------------- keys

/// An arrow key that an open menu has no use for goes to the bar the
/// menu is in: the menu closes and the next one of the bar opens, or the
/// one before, around the ends. `menu` is the menu the bar opened. False
/// if it is in no bar, or the bar has no other menu.
pub(crate) fn step(e: &Engine, menu: ObjectId, forward: bool) -> bool {
    let Some(item) = e
        .untrack(|| object(e, menu, Popup::PARENT.index))
        .filter(|i| menu_of(e, *i) == Some(menu))
    else {
        return false;
    };
    let Some(bar) = e
        .untrack(|| object(e, item, MenuBarItem::MENU_BAR.index))
        .filter(|b| is_a::<MenuBar>(e, *b))
    else {
        return false;
    };
    let Some(st) = state_of(e, bar) else {
        return false;
    };
    let items = items(e, bar);
    let Some(from) = items.iter().position(|i| *i == item) else {
        return false;
    };
    let n = items.len();
    let next = (1..n)
        .map(|k| {
            if forward {
                (from + k) % n
            } else {
                (from + n - k) % n
            }
        })
        .map(|i| items[i])
        .find_map(|i| menu_of(e, i).filter(|_| takes_input(e, i)).map(|m| (i, m)));
    let Some((to, to_menu)) = next else {
        return false;
    };
    let moved = close_quietly(e, &st, menu).and_then(|()| {
        set_current(e, bar, Some(to));
        open_under(e, to, to_menu)
    });
    check(e, "menu bar", moved);
    true
}

pub(crate) fn register(engine: &Engine) {
    // The keyboard an item has is the bar's (`activeFocus` of both, on
    // Qt 6.11.2).
    register_focus_scope(engine, MetaRef::Static(MenuBar::static_meta()));
}
