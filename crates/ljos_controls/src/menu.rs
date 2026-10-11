//! `Menu`, `MenuItem` and `MenuSeparator` (QtQuick.Templates).
//!
//! A menu is a popup whose content is a column of items. What is declared
//! in it becomes an item of the column: a `MenuItem` or any other item as
//! it is, an `Action` as an item made from the menu's `delegate`, and a
//! `Menu` as an item that opens it beside its parent.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-menu.html,
//! `-menuitem.html` and `-menuseparator.html`. Which key does what, what
//! a click, a hover and a sub-menu do and in which order the signals
//! come were read off a running Qt 6.11.2; `features/controls/
//! menu.feature` and `menu-pointer.feature` pin them.
//!
//! TODO(listview): the column is laid out here. Qt's is a `ListView` on
//! `contentModel`, which reads null until there is one, and a menu
//! taller than its window does not scroll.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_a11y::Role;
use ljos_engine::{
    Atom, ConnectionId, Cx, Engine, EngineError, ObjectFlags, ObjectId, OverrideContent,
    OverrideId, PropIndex, QmlObject, RestoreMode, Timer, Value, WriteKind, qml_impl,
};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::keys::key;
use ljos_quick::{
    Item, KeyEvent, follows_implicit_size, is_item, scene_transform, set_parent_item,
};
use ljos_render::kurbo;

use crate::action::Action;
use crate::button::{AbstractButton, Icon};
use crate::containers::{Page, host_append};
use crate::control::{Control, takes_input};
use crate::parts::{PartKind, manage_part};
use crate::popup::{self, POPUP, Popup};
use crate::util::{bind_default, check, flag, is_a, num, object, set_computed};

/// How long the pointer rests on an item before its sub-menu opens
/// (measured on Qt 6.11.2).
const SUB_MENU_DELAY_MS: i32 = 225;

// ------------------------------------------------------------- MenuItem

/// `MenuItem`: a button that is one line of a menu.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "MenuItem", extends = AbstractButton)]
pub struct MenuItem {
    #[qml(base)]
    pub button: AbstractButton,
    /// The item is the menu's current one.
    #[qml(notify)]
    pub highlighted: bool,
    /// What shows that the item opens a sub-menu.
    #[qml(notify)]
    pub arrow: Option<ObjectId>,
    /// The menu the item is in.
    #[qml(notify, readonly)]
    pub menu: Option<ObjectId>,
    /// The menu the item opens.
    #[qml(notify, readonly)]
    pub sub_menu: Option<ObjectId>,
    /// Stored: a style may use it to line up the texts of a menu.
    #[qml(notify)]
    pub text_padding: f64,
    #[qml(notify, readonly)]
    pub implicit_text_padding: f64,
}

impl Default for MenuItem {
    fn default() -> Self {
        let mut button = AbstractButton::default();
        // `Qt.StrongFocus`: the current item of an open menu has the
        // keyboard.
        button.control.focus_policy = 11;
        Self {
            button,
            highlighted: false,
            arrow: None,
            menu: None,
            sub_menu: None,
            text_padding: 0.0,
            implicit_text_padding: 0.0,
        }
    }
}

#[qml_impl]
impl MenuItem {
    /// The user chose the item.
    #[qml(signal)]
    fn triggered();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, item) = (cx.engine, cx.this);
        manage_part(e, item, MenuItem::ARROW.index, PartKind::Free)?;
        e.connect_by_name(item, "clicked", move |e, _| {
            if e.is_alive(item) {
                check(e, "triggered", e.emit_by_name(item, "triggered", &[]));
            }
        })?;
        Ok(())
    }
}

// -------------------------------------------------------- MenuSeparator

/// `MenuSeparator`: a line between two groups of items.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "MenuSeparator", extends = Control)]
pub struct MenuSeparator {
    #[qml(base)]
    pub control: Control,
}

#[qml_impl]
impl MenuSeparator {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        crate::a11y::defaults(cx.engine, cx.this, Role::Separator, None)
    }
}

// ----------------------------------------------------------------- Menu

/// An item of a menu and what the menu put on it.
#[derive(Debug)]
struct Entry {
    item: ObjectId,
    /// The action or the menu the menu made the item for.
    made_for: Option<ObjectId>,
    /// What the menu listens to on the item.
    conns: Vec<ConnectionId>,
    /// Where the menu holds the item's position, and its width unless
    /// the author gave it one.
    x: OverrideId,
    y: OverrideId,
    width: Option<OverrideId>,
}

/// What a menu remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct MenuState {
    entries: RefCell<Vec<Entry>>,
    /// The menu this one is a sub-menu of.
    parent_menu: Cell<Option<ObjectId>>,
    /// The sub-menu that is open beside this menu.
    open_sub: Cell<Option<ObjectId>>,
    /// The timer of the sub-menu the pointer rests on, and its item.
    timer: Cell<Option<ObjectId>>,
    waiting_for: Cell<Option<ObjectId>>,
    /// The menu is making an item from its delegate, which appends it.
    making: Cell<bool>,
    /// The item the menu gave the keyboard.
    focused: Cell<Option<ObjectId>>,
}

/// `Menu`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Menu", extends = Popup)]
pub struct Menu {
    #[qml(base)]
    pub popup: Popup,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify, readonly)]
    pub count: i32,
    /// Whether a sub-menu opens beside its parent (true) or in its place.
    #[qml(notify)]
    pub cascade: bool,
    /// How far a sub-menu that cascades lies over its parent.
    #[qml(notify)]
    pub overlap: f64,
    /// What an item for an `Action` or a `Menu` is made from.
    #[qml(notify)]
    pub delegate: Option<ObjectId>,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(group)]
    pub icon: Icon,
    /// See the module's TODO.
    #[qml(notify, readonly)]
    pub content_model: Value,
    #[qml(skip)]
    pub(crate) state: Rc<MenuState>,
}

impl Default for Menu {
    fn default() -> Self {
        // A menu takes the keyboard when it opens.
        let popup = Popup {
            focus: true,
            ..Popup::default()
        };
        Self {
            popup,
            title: Atom::EMPTY,
            count: 0,
            // Qt asks the platform; a desktop says yes.
            cascade: true,
            overlap: 0.0,
            delegate: None,
            current_index: -1,
            icon: Icon::default(),
            content_model: Value::Null,
            state: Rc::default(),
        }
    }
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
impl Menu {
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, index: Value) -> Value {
        or_null(index_arg(&index).and_then(|i| items(cx.engine, cx.this).get(i).copied()))
    }

    #[qml(invokable, name = "addItem")]
    fn add_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, item, None, Kind::Item)
    }

    #[qml(invokable, name = "insertItem")]
    fn insert_item(cx: &Cx, index: Value, item: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, item, index_arg(&index), Kind::Item)
    }

    #[qml(invokable, name = "moveItem")]
    fn move_item(cx: &Cx, from: Value, to: Value) -> Result<(), EngineError> {
        let (e, menu) = (cx.engine, cx.this);
        let (Some(st), Some(from), Some(to)) =
            (state_of(e, menu), index_arg(&from), index_arg(&to))
        else {
            return Ok(());
        };
        {
            let mut entries = st.entries.borrow_mut();
            if from >= entries.len() || to >= entries.len() || from == to {
                return Ok(());
            }
            let entry = entries.remove(from);
            entries.insert(to, entry);
        }
        changed(e, menu)
    }

    /// Takes the item out of the menu and destroys it.
    #[qml(invokable, name = "removeItem")]
    fn remove_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        let (e, menu) = (cx.engine, cx.this);
        let Some(item) = item else {
            return Ok(());
        };
        if leave(e, menu, item)?.is_some() {
            e.destroy(item);
            changed(e, menu)?;
        }
        Ok(())
    }

    /// Takes the item at `index` out of the menu and returns it.
    #[qml(invokable, name = "takeItem")]
    fn take_item(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        take(cx.engine, cx.this, index_arg(&index), Kind::Item)
    }

    #[qml(invokable, name = "menuAt")]
    fn menu_at(cx: &Cx, index: Value) -> Value {
        let e = cx.engine;
        or_null(
            index_arg(&index)
                .and_then(|i| items(e, cx.this).get(i).copied())
                .and_then(|item| sub_menu_of(e, item)),
        )
    }

    #[qml(invokable, name = "addMenu")]
    fn add_menu(cx: &Cx, menu: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, menu, None, Kind::Menu)
    }

    #[qml(invokable, name = "insertMenu")]
    fn insert_menu(cx: &Cx, index: Value, menu: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, menu, index_arg(&index), Kind::Menu)
    }

    #[qml(invokable, name = "removeMenu")]
    fn remove_menu(cx: &Cx, menu: Option<ObjectId>) -> Result<(), EngineError> {
        remove_made(cx.engine, cx.this, menu, Kind::Menu)
    }

    #[qml(invokable, name = "takeMenu")]
    fn take_menu(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        take(cx.engine, cx.this, index_arg(&index), Kind::Menu)
    }

    #[qml(invokable, name = "actionAt")]
    fn action_at(cx: &Cx, index: Value) -> Value {
        let e = cx.engine;
        or_null(
            index_arg(&index)
                .and_then(|i| items(e, cx.this).get(i).copied())
                .and_then(|item| action_of(e, item)),
        )
    }

    #[qml(invokable, name = "addAction")]
    fn add_action(cx: &Cx, action: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, action, None, Kind::Action)
    }

    #[qml(invokable, name = "insertAction")]
    fn insert_action(cx: &Cx, index: Value, action: Option<ObjectId>) -> Result<(), EngineError> {
        add(cx.engine, cx.this, action, index_arg(&index), Kind::Action)
    }

    #[qml(invokable, name = "removeAction")]
    fn remove_action(cx: &Cx, action: Option<ObjectId>) -> Result<(), EngineError> {
        remove_made(cx.engine, cx.this, action, Kind::Action)
    }

    #[qml(invokable, name = "takeAction")]
    fn take_action(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        take(cx.engine, cx.this, index_arg(&index), Kind::Action)
    }

    /// Opens the menu: `popup([parent,] [x, y | point,] [item])`. Without
    /// a position it opens where the pointer last was; with an item, so
    /// that the item is there and current.
    #[qml(invokable)]
    fn popup(cx: &Cx, a: Value, b: Value, c: Value, d: Value) -> Result<(), EngineError> {
        popup_at(cx.engine, cx.this, &[a, b, c, d])
    }

    /// Closes the menu and every menu it is a sub-menu of.
    #[qml(invokable)]
    fn dismiss(cx: &Cx) -> Result<(), EngineError> {
        close_chain(cx.engine, cx.this)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_menu(cx.engine, cx.this)
    }
}

/// What a method of the menu is about: its items, or the items that
/// stand for an action or for a sub-menu.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Item,
    Action,
    Menu,
}

fn state_of(engine: &Engine, menu: ObjectId) -> Option<Rc<MenuState>> {
    engine.with::<Menu, _>(menu, |m| m.state.clone()).ok()
}

/// The items of a menu, in order.
fn items(engine: &Engine, menu: ObjectId) -> Vec<ObjectId> {
    state_of(engine, menu).map_or_else(Vec::new, |s| {
        s.entries
            .borrow()
            .iter()
            .map(|en| en.item)
            .filter(|i| engine.is_alive(*i))
            .collect()
    })
}

/// Where `item` is among the items of its menu that can be chosen, from
/// 1, and how many there are: what an assistive technology reads as
/// "2 of 3". A separator is not one of them. Read inside an effect, it
/// follows the menu's items.
pub(crate) fn place_of_item(engine: &Engine, item: ObjectId) -> Option<(usize, usize)> {
    if !is_a::<MenuItem>(engine, item) {
        return None;
    }
    let menu = object(engine, item, MenuItem::MENU.index).filter(|m| is_a::<Menu>(engine, *m))?;
    // Changes whenever an item joins or leaves.
    let _ = engine.read(menu, Menu::COUNT.index);
    let choices: Vec<ObjectId> = items(engine, menu)
        .into_iter()
        .filter(|i| is_a::<MenuItem>(engine, *i))
        .collect();
    let at = choices.iter().position(|i| *i == item)?;
    Some((at + 1, choices.len()))
}

fn sub_menu_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    if !is_a::<MenuItem>(engine, item) {
        return None;
    }
    engine
        .untrack(|| object(engine, item, MenuItem::SUB_MENU.index))
        .filter(|m| is_a::<Menu>(engine, *m))
}

fn action_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    if !is_a::<AbstractButton>(engine, item) {
        return None;
    }
    engine
        .untrack(|| object(engine, item, AbstractButton::ACTION.index))
        .filter(|a| is_a::<Action>(engine, *a))
}

fn current_index(engine: &Engine, menu: ObjectId) -> i32 {
    engine
        .untrack(|| engine.read(menu, Menu::CURRENT_INDEX.index))
        .to_number() as i32
}

fn current_item(engine: &Engine, menu: ObjectId) -> Option<ObjectId> {
    usize::try_from(current_index(engine, menu))
        .ok()
        .and_then(|i| items(engine, menu).get(i).copied())
}

fn set_current(engine: &Engine, menu: ObjectId, index: i32) {
    let r = popup::put(engine, menu, Menu::CURRENT_INDEX.index, Value::Int(index));
    check(engine, "currentIndex", r);
}

fn cascades(engine: &Engine, menu: ObjectId) -> bool {
    engine.untrack(|| flag(engine, menu, Menu::CASCADE.index))
}

fn is_open(engine: &Engine, menu: ObjectId) -> bool {
    engine.untrack(|| flag(engine, menu, Popup::VISIBLE.index))
}

/// Whether the keyboard and the pointer can make `item` the current one:
/// a separator and a disabled item are passed over.
fn can_be_current(engine: &Engine, item: ObjectId) -> bool {
    is_a::<MenuItem>(engine, item) && takes_input(engine, item)
}

fn first_choice(engine: &Engine, menu: ObjectId) -> i32 {
    items(engine, menu)
        .iter()
        .position(|i| can_be_current(engine, *i))
        .map_or(-1, |i| i as i32)
}

// ------------------------------------------------------------- children

/// The append hook of a menu's `contentData`.
pub(crate) fn append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let (e, menu) = (cx.engine, cx.this);
    let complete = e
        .flags(menu)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if !complete {
        // Taken in when the menu is complete, when its delegate is known.
        return host_append(cx, child, POPUP);
    }
    if state_of(e, menu).is_some_and(|s| s.making.get()) {
        // The menu takes in what it makes itself.
        return Ok(());
    }
    take_in(e, menu, child, None)?;
    changed(e, menu)
}

fn init_menu(e: &Engine, menu: ObjectId) -> Result<(), EngineError> {
    let declared: Vec<ObjectId> = match e.untrack(|| e.read(menu, POPUP.content_data)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    };
    for child in declared {
        if e.is_alive(child) {
            take_in(e, menu, child, None)?;
        }
    }
    sync_count(e, menu)?;

    e.effect_in(menu, move |e| check(e, "menu", layout(e, menu)))?;
    e.watch(menu, Menu::CURRENT_INDEX.index, move |e, _| {
        apply_current(e, menu);
    })?;
    e.connect_by_name(menu, "opened", move |e, _| apply_current(e, menu))?;
    e.connect_by_name(menu, "aboutToHide", move |e, _| about_to_hide(e, menu))?;
    apply_current(e, menu);

    if let Some(item) = popup::item_of(e, menu) {
        // The popup's item is a page, which a title names.
        popup::follow(e, item, Page::TITLE.index, menu, Menu::TITLE.index)?;
    }
    popup::set_role(e, menu, Role::PopupMenu)
}

/// Makes `obj` an item of the menu, the `at`-th or the last: an item as
/// it is, an action or a menu as an item made for it. Anything else is
/// only kept.
fn take_in(
    e: &Engine,
    menu: ObjectId,
    obj: ObjectId,
    at: Option<usize>,
) -> Result<(), EngineError> {
    if is_item(e, obj) {
        return join(e, menu, obj, at, None);
    }
    let for_action = is_a::<Action>(e, obj);
    if !for_action && !is_a::<Menu>(e, obj) {
        return keep(e, menu, obj);
    }
    if obj == menu {
        return Ok(());
    }
    let item = make_item(e, menu)?;
    if for_action {
        e.write(
            item,
            AbstractButton::ACTION.index,
            Value::Object(obj),
            WriteKind::Imperative,
        )?;
    } else if is_a::<MenuItem>(e, item) {
        set_computed(e, item, MenuItem::SUB_MENU.index, Value::Object(obj))?;
        e.bind(item, AbstractButton::TEXT.index, move |e| {
            if e.is_alive(obj) {
                e.read(obj, Menu::TITLE.index)
            } else {
                e.untrack(|| e.read(item, AbstractButton::TEXT.index))
            }
        })?;
        // A sub-menu is placed from its item.
        e.write(
            obj,
            Popup::PARENT.index,
            Value::Object(item),
            WriteKind::Imperative,
        )?;
        // And opens the way its parent does until it is told otherwise.
        bind_default(e, obj, Menu::CASCADE.index, move |e| {
            if e.is_alive(menu) {
                e.read(menu, Menu::CASCADE.index)
            } else {
                e.untrack(|| e.read(obj, Menu::CASCADE.index))
            }
        })?;
    }
    join(e, menu, item, at, Some(obj))
}

/// Lists `obj` in the menu's `contentData` if it is not there.
fn keep(e: &Engine, menu: ObjectId, obj: ObjectId) -> Result<(), EngineError> {
    let known = matches!(
        e.untrack(|| e.read(menu, POPUP.content_data)),
        Value::List(l) if l.iter().any(|v| v.as_object() == Some(obj))
    );
    if !known {
        e.list_push(menu, POPUP.content_data, obj)?;
        if is_item(e, obj) {
            e.list_push(menu, POPUP.content_children, obj)?;
        }
    }
    Ok(())
}

/// An item from the menu's `delegate`, or a plain `MenuItem` if that
/// makes none.
fn make_item(e: &Engine, menu: ObjectId) -> Result<ObjectId, EngineError> {
    let delegate = e.untrack(|| object(e, menu, Menu::DELEGATE.index));
    if let (Some(d), Some(st)) = (delegate, state_of(e, menu)) {
        st.making.set(true);
        let made = e.component_create(d, Some(menu), Vec::new());
        st.making.set(false);
        match made {
            Ok(item) if is_item(e, item) => return Ok(item),
            Ok(other) => {
                e.destroy(other);
            }
            Err(err) => check(e, "delegate", Err(err)),
        }
    }
    let item = e.create_native(MenuItem::default(), Some(menu))?;
    e.complete(item)?;
    Ok(item)
}

/// Takes `item` into the menu as its `at`-th item, or its last. An item
/// that is in the menu already moves there.
fn join(
    e: &Engine,
    menu: ObjectId,
    item: ObjectId,
    at: Option<usize>,
    made_for: Option<ObjectId>,
) -> Result<(), EngineError> {
    let Some(st) = state_of(e, menu) else {
        return Ok(());
    };
    {
        let mut entries = st.entries.borrow_mut();
        if let Some(from) = entries.iter().position(|en| en.item == item) {
            if let Some(to) = at {
                let entry = entries.remove(from);
                let to = to.min(entries.len());
                entries.insert(to, entry);
            }
            return Ok(());
        }
    }
    keep(e, menu, item)?;
    if let Some(ci) = e.untrack(|| object(e, menu, POPUP.content_item)) {
        set_parent_item(e, item, Some(ci))?;
    }
    let own_width = e.untrack(|| !follows_implicit_size(e, item, true));
    let hold = |prop: PropIndex| {
        let now = e.untrack(|| e.read(item, prop));
        e.push_override(item, prop, OverrideContent::Value(now))
    };
    let mut entry = Entry {
        item,
        made_for,
        conns: Vec::new(),
        x: hold(Item::X.index)?,
        y: hold(Item::Y.index)?,
        width: (!own_width).then(|| hold(Item::WIDTH.index)).transpose()?,
    };
    if is_a::<MenuItem>(e, item) {
        set_computed(e, item, MenuItem::MENU.index, Value::Object(menu))?;
        entry.conns = vec![
            e.connect_by_name(item, "triggered", move |e, _| {
                check(e, "menu", item_triggered(e, menu, item));
            })?,
            e.connect_by_name(item, "pressed", move |e, _| item_pressed(e, menu, item))?,
            e.connect_by_name(item, "hoveredChanged", move |e, _| {
                item_hovered(e, menu, item);
            })?,
        ];
        if let Some(sub) = sub_menu_of(e, item)
            && let Some(sub_state) = state_of(e, sub)
        {
            sub_state.parent_menu.set(Some(menu));
        }
    }
    let mut entries = st.entries.borrow_mut();
    let at = at.map_or(entries.len(), |i| i.min(entries.len()));
    entries.insert(at, entry);
    Ok(())
}

/// Takes `item` out of the menu: it is no child of it any more and keeps
/// what it was given. None if it was not in the menu.
fn leave(e: &Engine, menu: ObjectId, item: ObjectId) -> Result<Option<Entry>, EngineError> {
    let Some(st) = state_of(e, menu) else {
        return Ok(None);
    };
    let entry = {
        let mut entries = st.entries.borrow_mut();
        match entries.iter().position(|en| en.item == item) {
            Some(i) => entries.remove(i),
            None => return Ok(None),
        }
    };
    for conn in &entry.conns {
        e.disconnect(*conn);
    }
    if st.waiting_for.get() == Some(item) {
        stop_timer(e, &st);
    }
    if st.focused.get() == Some(item) {
        st.focused.set(None);
    }
    if !e.is_alive(item) {
        return Ok(Some(entry));
    }
    if let Some(sub) = sub_menu_of(e, item) {
        if st.open_sub.get() == Some(sub) {
            popup::close(e, sub)?;
        }
        if let Some(sub_state) = state_of(e, sub) {
            sub_state.parent_menu.set(None);
        }
    }
    for (prop, layer) in [
        (Item::X.index, Some(entry.x)),
        (Item::Y.index, Some(entry.y)),
        (Item::WIDTH.index, entry.width),
    ] {
        if let Some(layer) = layer {
            let _ = e.remove_override(item, prop, layer, RestoreMode::BindingOrValue);
        }
    }
    e.list_remove(menu, POPUP.content_data, item)?;
    e.list_remove(menu, POPUP.content_children, item)?;
    set_parent_item(e, item, None)?;
    if is_a::<MenuItem>(e, item) {
        set_computed(e, item, MenuItem::MENU.index, Value::Null)?;
        popup::put(e, item, MenuItem::HIGHLIGHTED.index, Value::Bool(false))?;
    }
    Ok(Some(entry))
}

/// `addItem`, `addAction`, `addMenu` and their `insert` forms.
fn add(
    e: &Engine,
    menu: ObjectId,
    obj: Option<ObjectId>,
    at: Option<usize>,
    kind: Kind,
) -> Result<(), EngineError> {
    let Some(obj) = obj.filter(|o| e.is_alive(*o)) else {
        return Ok(());
    };
    let fits = match kind {
        Kind::Item => is_item(e, obj),
        Kind::Action => is_a::<Action>(e, obj),
        Kind::Menu => is_a::<Menu>(e, obj),
    };
    if !fits {
        return Ok(());
    }
    take_in(e, menu, obj, at)?;
    changed(e, menu)
}

/// `takeItem`, `takeAction` and `takeMenu`: what is returned is the
/// item, or what the item stood for, and null if the item at `index`
/// stands for no such thing.
fn take(
    e: &Engine,
    menu: ObjectId,
    index: Option<usize>,
    kind: Kind,
) -> Result<Value, EngineError> {
    let Some(item) = index.and_then(|i| items(e, menu).get(i).copied()) else {
        return Ok(Value::Null);
    };
    let taken = match kind {
        Kind::Item => Some(item),
        Kind::Action => action_of(e, item),
        Kind::Menu => sub_menu_of(e, item),
    };
    let Some(taken) = taken else {
        return Ok(Value::Null);
    };
    let entry = leave(e, menu, item)?;
    // The item of an action or of a menu was the menu's own.
    if kind != Kind::Item && entry.is_some_and(|en| en.made_for.is_some()) {
        e.destroy(item);
    }
    changed(e, menu)?;
    Ok(Value::Object(taken))
}

/// `removeAction` and `removeMenu`.
fn remove_made(
    e: &Engine,
    menu: ObjectId,
    target: Option<ObjectId>,
    kind: Kind,
) -> Result<(), EngineError> {
    let Some(target) = target else {
        return Ok(());
    };
    let found = items(e, menu).into_iter().find(|item| {
        let stands_for = match kind {
            Kind::Action => action_of(e, *item),
            _ => sub_menu_of(e, *item),
        };
        stands_for == Some(target)
    });
    let Some(item) = found else {
        return Ok(());
    };
    if leave(e, menu, item)?.is_some_and(|en| en.made_for.is_some()) {
        e.destroy(item);
    }
    changed(e, menu)
}

fn sync_count(e: &Engine, menu: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, menu) else {
        return Ok(());
    };
    st.entries.borrow_mut().retain(|en| e.is_alive(en.item));
    let count = st.entries.borrow().len() as i32;
    set_computed(e, menu, Menu::COUNT.index, Value::Int(count))
}

/// The items of the menu changed: their number, where they are and
/// which of them is the current one.
fn changed(e: &Engine, menu: ObjectId) -> Result<(), EngineError> {
    sync_count(e, menu)?;
    e.untrack(|| layout(e, menu))?;
    apply_current(e, menu);
    Ok(())
}

// --------------------------------------------------------------- layout

/// Puts the items below each other, as wide as the menu's content. What
/// it reads is what moves an item when it runs in an effect.
fn layout(e: &Engine, menu: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, menu) else {
        return Ok(());
    };
    let _ = e.read(menu, Menu::COUNT.index);
    let Some(ci) = object(e, menu, POPUP.content_item).filter(|c| is_item(e, *c)) else {
        return Ok(());
    };
    let width = num(e, ci, Item::WIDTH.index);
    let rows: Vec<(ObjectId, f64, OverrideId, OverrideId, Option<OverrideId>)> = st
        .entries
        .borrow()
        .iter()
        .filter(|en| e.is_alive(en.item))
        .map(|en| {
            (
                en.item,
                num(e, en.item, Item::HEIGHT.index),
                en.x,
                en.y,
                en.width,
            )
        })
        .collect();
    e.untrack(|| {
        let mut top = 0.0;
        for (item, height, x, y, w) in rows {
            let put = |prop: PropIndex, layer: OverrideId, v: f64| {
                if e.read(item, prop).to_number() == v {
                    return Ok(());
                }
                e.write_internal(item, prop, Value::Real(v), WriteKind::Override(layer))
            };
            put(Item::X.index, x, 0.0)?;
            put(Item::Y.index, y, top)?;
            if let Some(w) = w {
                put(Item::WIDTH.index, w, width)?;
            }
            top += height;
        }
        Ok(())
    })
}

/// How big the content of a menu is along one axis, for a popup that is
/// a menu: as high as its items together, and no width of its own.
pub(crate) fn content_extent(e: &Engine, popup: ObjectId, horizontal: bool) -> Option<f64> {
    let st = state_of(e, popup)?;
    if horizontal {
        return Some(0.0);
    }
    let _ = e.read(popup, Menu::COUNT.index);
    let items: Vec<ObjectId> = st.entries.borrow().iter().map(|en| en.item).collect();
    Some(
        items
            .into_iter()
            .filter(|i| e.is_alive(*i))
            .map(|i| num(e, i, Item::HEIGHT.index))
            .sum(),
    )
}

// ------------------------------------------------------- the current item

/// Makes what the items show, and where the keyboard is, agree with
/// `currentIndex`.
fn apply_current(e: &Engine, menu: ObjectId) {
    let Some(st) = state_of(e, menu) else {
        return;
    };
    let current = current_item(e, menu);
    for item in items(e, menu) {
        if is_a::<MenuItem>(e, item) {
            let on = Value::Bool(Some(item) == current);
            let r = popup::put(e, item, MenuItem::HIGHLIGHTED.index, on);
            check(e, "highlighted", r);
        }
    }
    if st.waiting_for.get().is_some() && st.waiting_for.get() != current {
        stop_timer(e, &st);
    }
    let has_keyboard = popup::is_shown(e, menu) && e.untrack(|| flag(e, menu, Popup::FOCUS.index));
    if !has_keyboard {
        return;
    }
    match current {
        Some(item) => {
            st.focused.set(Some(item));
            force_active_focus(e, item, FocusReason::Other);
        }
        None => {
            // Back to the menu itself, which is what has the keyboard
            // when no item is current.
            if let Some(old) = st.focused.take().filter(|o| e.is_alive(*o))
                && e.untrack(|| flag(e, old, Item::FOCUS.index))
            {
                let r = e.write(
                    old,
                    Item::FOCUS.index,
                    Value::Bool(false),
                    WriteKind::Imperative,
                );
                check(e, "focus", r);
            }
        }
    }
}

fn about_to_hide(e: &Engine, menu: ObjectId) {
    let Some(st) = state_of(e, menu) else {
        return;
    };
    stop_timer(e, &st);
    // The deepest menu goes first, so that the keyboard goes back one
    // menu at a time.
    if let Some(sub) = st.open_sub.take().filter(|s| e.is_alive(*s)) {
        check(e, "close", popup::close(e, sub));
    }
    if let Some(parent) = st.parent_menu.get()
        && let Some(parent_state) = state_of(e, parent)
        && parent_state.open_sub.get() == Some(menu)
    {
        parent_state.open_sub.set(None);
    }
    // A menu that takes the place of its parent comes back as it was
    // left; one that cascades starts anew (checked on Qt).
    if cascades(e, menu) {
        set_current(e, menu, -1);
    }
}

/// Closes `menu`, what is open beside it and what it is a sub-menu of.
fn close_chain(e: &Engine, menu: ObjectId) -> Result<(), EngineError> {
    let mut subs = Vec::new();
    let mut cur = menu;
    for _ in 0..64 {
        match state_of(e, cur).and_then(|s| s.open_sub.get()) {
            Some(sub) if e.is_alive(sub) => {
                subs.push(sub);
                cur = sub;
            }
            _ => break,
        }
    }
    let mut chain = vec![menu];
    let mut cur = menu;
    for _ in 0..64 {
        match state_of(e, cur).and_then(|s| s.parent_menu.get()) {
            Some(parent) if e.is_alive(parent) => {
                chain.push(parent);
                cur = parent;
            }
            _ => break,
        }
    }
    for m in subs.into_iter().rev().chain(chain) {
        if e.is_alive(m) {
            popup::close(e, m)?;
        }
    }
    Ok(())
}

fn item_triggered(e: &Engine, menu: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    match sub_menu_of(e, item) {
        Some(sub) => open_sub(e, menu, item, sub, true),
        None => close_chain(e, menu),
    }
}

fn index_of(e: &Engine, menu: ObjectId, item: ObjectId) -> Option<i32> {
    items(e, menu)
        .iter()
        .position(|i| *i == item)
        .map(|i| i as i32)
}

fn item_pressed(e: &Engine, menu: ObjectId, item: ObjectId) {
    if let Some(i) = index_of(e, menu, item) {
        set_current(e, menu, i);
    }
}

/// The pointer came over an item, or left it. Leaving changes nothing:
/// the item stays current until another is.
fn item_hovered(e: &Engine, menu: ObjectId, item: ObjectId) {
    let over = e.untrack(|| flag(e, item, Control::HOVERED.index));
    // A menu that does not cascade is one for a finger, which does not
    // hover (checked on Qt).
    if !over || !cascades(e, menu) || !takes_input(e, item) {
        return;
    }
    // The click that closes the menu ends over the item it chose.
    if !popup::is_shown(e, menu) {
        return;
    }
    let (Some(st), Some(index)) = (state_of(e, menu), index_of(e, menu, item)) else {
        return;
    };
    let sub = sub_menu_of(e, item);
    if let Some(open) = st.open_sub.get()
        && Some(open) != sub
    {
        check(e, "close", popup::close(e, open));
    }
    set_current(e, menu, index);
    if let Some(sub) = sub
        && !is_open(e, sub)
    {
        st.waiting_for.set(Some(item));
        start_timer(e, menu, &st);
    }
}

// ------------------------------------------------------------- sub-menus

/// Opens the sub-menu of `item`: beside the item if the menu cascades,
/// in the place of the menu if not.
fn open_sub(
    e: &Engine,
    menu: ObjectId,
    item: ObjectId,
    sub: ObjectId,
    select_first: bool,
) -> Result<(), EngineError> {
    let Some(st) = state_of(e, menu) else {
        return Ok(());
    };
    stop_timer(e, &st);
    if is_open(e, sub) {
        return Ok(());
    }
    e.untrack(|| {
        let (sw, sh) = (
            num(e, sub, Popup::WIDTH.index),
            num(e, sub, Popup::HEIGHT.index),
        );
        if cascades(e, menu) {
            if let Some(other) = st.open_sub.get().filter(|o| *o != sub) {
                popup::close(e, other)?;
            }
            let overlap = num(e, menu, Menu::OVERLAP.index);
            let beside = num(e, item, Item::WIDTH.index) - overlap;
            let window_width = popup::item_of(e, menu)
                .and_then(|i| e.visual_parent(i))
                .map_or(0.0, |o| num(e, o, Item::WIDTH.index));
            let right_edge = (scene_transform(e, item) * kurbo::Point::new(beside + sw, 0.0)).x;
            let other_side = flag(e, menu, Popup::MIRRORED.index)
                || (window_width > 0.0 && right_edge > window_width);
            let x = if other_side { overlap - sw } else { beside };
            let y = -num(e, sub, Popup::TOP_PADDING.index);
            popup::move_to(e, sub, x, y)?;
            let first = if select_first {
                first_choice(e, sub)
            } else {
                -1
            };
            set_current(e, sub, first);
            st.open_sub.set(Some(sub));
            popup::open(e, sub)
        } else {
            let Some(mine) = popup::item_of(e, menu) else {
                return Ok(());
            };
            let corner = scene_transform(e, mine) * kurbo::Point::ZERO;
            let (mw, mh) = (
                num(e, mine, Item::WIDTH.index),
                num(e, mine, Item::HEIGHT.index),
            );
            popup::close(e, menu)?;
            let middle = kurbo::Point::new(corner.x + (mw - sw) / 2.0, corner.y + (mh - sh) / 2.0);
            let local = scene_transform(e, item).inverse() * middle;
            popup::move_to(e, sub, local.x, local.y)?;
            set_current(e, sub, first_choice(e, sub));
            popup::open(e, sub)
        }
    })
}

fn start_timer(e: &Engine, menu: ObjectId, st: &MenuState) {
    let timer = match st.timer.get().filter(|t| e.is_alive(*t)) {
        Some(t) => t,
        None => {
            // An engine timer: it runs on the engine's clock and nothing
            // polls.
            let made = e.create_native(Timer::default(), Some(menu)).and_then(|t| {
                e.complete(t)?;
                e.connect_by_name(t, "triggered", move |e, _| timer_fired(e, menu))?;
                Ok(t)
            });
            match made {
                Ok(t) => {
                    st.timer.set(Some(t));
                    t
                }
                Err(err) => {
                    check(e, "timer", Err(err));
                    return;
                }
            }
        }
    };
    let r = e
        .set(timer, Timer::INTERVAL, SUB_MENU_DELAY_MS)
        .and_then(|()| e.invoke_by_name(timer, "restart", &[]).map(|_| ()));
    check(e, "timer", r);
}

fn stop_timer(e: &Engine, st: &MenuState) {
    st.waiting_for.set(None);
    if let Some(t) = st.timer.get().filter(|t| e.is_alive(*t)) {
        let _ = e.set(t, Timer::RUNNING, false);
    }
}

fn timer_fired(e: &Engine, menu: ObjectId) {
    let Some(st) = state_of(e, menu) else {
        return;
    };
    let Some(item) = st.waiting_for.take().filter(|i| e.is_alive(*i)) else {
        return;
    };
    if !popup::is_shown(e, menu) || current_item(e, menu) != Some(item) {
        return;
    }
    if let Some(sub) = sub_menu_of(e, item) {
        // Opened by resting on its item: nothing in it is chosen yet.
        check(e, "menu", open_sub(e, menu, item, sub, false));
    }
}

// ----------------------------------------------------------------- keys

/// The next item the keyboard can go to from `from`, one way or the
/// other.
fn step(e: &Engine, menu: ObjectId, from: i32, forward: bool, wrap: bool) -> i32 {
    let items = items(e, menu);
    let n = items.len() as i32;
    let mut i = from;
    for _ in 0..n {
        i += if forward { 1 } else { -1 };
        if i < 0 || i >= n {
            if !wrap {
                return from;
            }
            i = if forward { 0 } else { n - 1 };
        }
        if can_be_current(e, items[i as usize]) {
            return i;
        }
    }
    from
}

/// A key pressed in a popup that may be a menu. True if the menu took
/// it.
pub(crate) fn key_press(e: &Engine, menu: ObjectId, ev: &KeyEvent) -> bool {
    let Some(st) = state_of(e, menu) else {
        return false;
    };
    let current = current_index(e, menu);
    let mirrored = e.untrack(|| flag(e, menu, Popup::MIRRORED.index));
    let (into, back) = if mirrored {
        (key::LEFT, key::RIGHT)
    } else {
        (key::RIGHT, key::LEFT)
    };
    match ev.key {
        key::DOWN => {
            set_current(e, menu, step(e, menu, current, true, false));
            true
        }
        key::UP => {
            // From no item there is nowhere to go up to.
            if current >= 0 {
                set_current(e, menu, step(e, menu, current, false, false));
            }
            true
        }
        key::TAB => {
            set_current(e, menu, step(e, menu, current, true, true));
            true
        }
        key::BACKTAB => {
            let from = if current < 0 {
                items(e, menu).len() as i32
            } else {
                current
            };
            let to = step(e, menu, from, false, true);
            set_current(e, menu, if to == from { current } else { to });
            true
        }
        key::RETURN | key::ENTER | key::SPACE => match current_item(e, menu) {
            Some(item) => {
                let r = e.invoke_by_name(item, "click", &[]).map(|_| ());
                check(e, "click", r);
                true
            }
            None => false,
        },
        k if k == into => {
            let Some(item) = current_item(e, menu) else {
                return false;
            };
            match sub_menu_of(e, item) {
                Some(sub) if takes_input(e, item) => {
                    check(e, "menu", open_sub(e, menu, item, sub, true));
                    true
                }
                _ => false,
            }
        }
        k if k == back => {
            let Some(parent) = st.parent_menu.get().filter(|p| e.is_alive(*p)) else {
                return false;
            };
            // What a sub-menu took the place of comes back.
            let reopen = !is_open(e, parent);
            let closed = popup::close(e, menu).and_then(|()| {
                if reopen {
                    popup::open(e, parent)
                } else {
                    Ok(())
                }
            });
            check(e, "menu", closed);
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- popup()

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Int(_) | Value::Real(_))
}

fn popup_at(e: &Engine, menu: ObjectId, args: &[Value; 4]) -> Result<(), EngineError> {
    let entries = items(e, menu);
    let mut next = 0;
    // An item that is not one of the menu's is what the menu opens from.
    if let Some(parent) = args[0]
        .as_object()
        .filter(|o| is_item(e, *o) && !entries.contains(o))
    {
        e.write(
            menu,
            Popup::PARENT.index,
            Value::Object(parent),
            WriteKind::Imperative,
        )?;
        next = 1;
    }
    let mut at = None;
    if let Value::Point(p) = &args[next] {
        at = Some((p.x, p.y));
        next += 1;
    } else if next + 1 < args.len() && is_number(&args[next]) && is_number(&args[next + 1]) {
        at = Some((args[next].to_number(), args[next + 1].to_number()));
        next += 2;
    }
    let (x, mut y) = at.unwrap_or_else(|| {
        // Where the pointer last was, seen from the parent.
        let parent = e
            .untrack(|| object(e, menu, Popup::PARENT.index))
            .filter(|p| is_item(e, *p));
        match (popup::last_pointer(e, menu), parent) {
            (Some(scene), Some(p)) => {
                let local = e.untrack(|| scene_transform(e, p)).inverse() * scene;
                (local.x, local.y)
            }
            _ => (0.0, 0.0),
        }
    });
    let chosen = args
        .get(next)
        .and_then(Value::as_object)
        .and_then(|o| entries.iter().position(|i| *i == o));
    if let Some(i) = chosen {
        // The item, not the top of the menu, is at the position.
        let above: f64 = entries[..i]
            .iter()
            .map(|item| e.untrack(|| num(e, *item, Item::HEIGHT.index)))
            .sum();
        y -= above + e.untrack(|| num(e, menu, Popup::TOP_PADDING.index));
    }
    popup::move_to(e, menu, x, y)?;
    set_current(e, menu, chosen.map_or(-1, |i| i as i32));
    popup::open(e, menu)
}
