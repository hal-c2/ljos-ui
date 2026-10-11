//! `Dialog` and `DialogButtonBox` (QtQuick.Templates).
//!
//! A dialog is a popup with a title above its content and a row of
//! buttons below it, and it closes with a result. The row is a
//! `DialogButtonBox`: each button in it has a role, the role says where
//! in the row the platform puts the button and what a click on it means.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-dialog.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-dialogbuttonbox.html. The order
//! of the roles on each platform, the sizes and the order of the signals
//! were read off a running `qml6` (Qt 6.11.2); `features/controls/
//! dialog.feature` and `dialog-button-box.feature` pin them.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectFlags, ObjectId, OverrideContent,
    OverrideId, PropIndex, QmlList, QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::{Item, follows_implicit_size, set_parent_item};

use crate::button::{AbstractButton, Button};
use crate::containers::{Host, Page, host_append, host_complete};
use crate::control::Control;
use crate::popup::{self, Popup};
use crate::util::{bind_default, check, is_a, num, object, set_computed};

/// `DialogButtonBox.ButtonRole`.
pub(crate) mod role {
    pub const INVALID: i32 = -1;
    pub const ACCEPT: i32 = 0;
    pub const REJECT: i32 = 1;
    pub const DESTRUCTIVE: i32 = 2;
    pub const ACTION: i32 = 3;
    pub const HELP: i32 = 4;
    pub const YES: i32 = 5;
    pub const NO: i32 = 6;
    pub const RESET: i32 = 7;
    pub const APPLY: i32 = 8;
}

/// `DialogButtonBox.ButtonLayout`.
mod layout {
    pub const WIN: i32 = 0;
    pub const MAC: i32 = 1;
    pub const KDE: i32 = 2;
    pub const GNOME: i32 = 3;
    pub const ANDROID: i32 = 4;
}

/// The standard buttons: flag, the name of the flag, the text and the
/// role, in the order a box makes them.
const STANDARD: [(i32, &str, &str, i32); 18] = [
    (0x0000_0400, "Ok", "OK", role::ACCEPT),
    (0x0000_0800, "Save", "Save", role::ACCEPT),
    (0x0000_1000, "SaveAll", "Save All", role::ACCEPT),
    (0x0000_2000, "Open", "Open", role::ACCEPT),
    (0x0000_4000, "Yes", "Yes", role::YES),
    (0x0000_8000, "YesToAll", "Yes to All", role::YES),
    (0x0001_0000, "No", "No", role::NO),
    (0x0002_0000, "NoToAll", "No to All", role::NO),
    (0x0004_0000, "Abort", "Abort", role::REJECT),
    (0x0008_0000, "Retry", "Retry", role::ACCEPT),
    (0x0010_0000, "Ignore", "Ignore", role::ACCEPT),
    (0x0020_0000, "Close", "Close", role::REJECT),
    (0x0040_0000, "Cancel", "Cancel", role::REJECT),
    (
        0x0080_0000,
        "Discard",
        "Close without Saving",
        role::DESTRUCTIVE,
    ),
    (0x0100_0000, "Help", "Help", role::HELP),
    (0x0200_0000, "Apply", "Apply", role::APPLY),
    (0x0400_0000, "Reset", "Reset", role::RESET),
    (
        0x0800_0000,
        "RestoreDefaults",
        "Restore Defaults",
        role::RESET,
    ),
];

/// The context Qt's own catalogues have the texts of the standard
/// buttons in, so that a Qt translation of them is found.
const TEXT_CONTEXT: &str = "QPlatformTheme";

fn standard_button_enum() -> (&'static str, Vec<(&'static str, i64)>) {
    let mut keys = vec![("NoButton", 0)];
    keys.extend(
        STANDARD
            .iter()
            .map(|(flag, name, ..)| (*name, i64::from(*flag))),
    );
    ("StandardButton", keys)
}

fn box_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("Position", vec![("Header", 0), ("Footer", 1)]),
        (
            "ButtonLayout",
            vec![
                ("WinLayout", i64::from(layout::WIN)),
                ("MacLayout", i64::from(layout::MAC)),
                ("KdeLayout", i64::from(layout::KDE)),
                ("GnomeLayout", i64::from(layout::GNOME)),
                ("AndroidLayout", i64::from(layout::ANDROID)),
            ],
        ),
        (
            "ButtonRole",
            vec![
                ("InvalidRole", i64::from(role::INVALID)),
                ("AcceptRole", i64::from(role::ACCEPT)),
                ("RejectRole", i64::from(role::REJECT)),
                ("DestructiveRole", i64::from(role::DESTRUCTIVE)),
                ("ActionRole", i64::from(role::ACTION)),
                ("HelpRole", i64::from(role::HELP)),
                ("YesRole", i64::from(role::YES)),
                ("NoRole", i64::from(role::NO)),
                ("ResetRole", i64::from(role::RESET)),
                ("ApplyRole", i64::from(role::APPLY)),
            ],
        ),
        standard_button_enum(),
    ]
}

fn dialog_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        ("StandardCode", vec![("Rejected", 0), ("Accepted", 1)]),
        standard_button_enum(),
    ]
}

/// The order of the roles in a row, by `ButtonLayout`. A button whose
/// role is none of them comes last.
fn role_order(button_layout: i32) -> [i32; 9] {
    use role::{ACCEPT, ACTION, APPLY, DESTRUCTIVE, HELP, NO, REJECT, RESET, YES};
    match button_layout {
        layout::WIN => [
            RESET,
            YES,
            ACCEPT,
            DESTRUCTIVE,
            NO,
            ACTION,
            REJECT,
            APPLY,
            HELP,
        ],
        layout::MAC => [
            HELP,
            RESET,
            APPLY,
            ACTION,
            DESTRUCTIVE,
            REJECT,
            ACCEPT,
            NO,
            YES,
        ],
        layout::KDE => [
            HELP,
            RESET,
            YES,
            NO,
            ACTION,
            ACCEPT,
            APPLY,
            DESTRUCTIVE,
            REJECT,
        ],
        layout::ANDROID => [
            HELP,
            RESET,
            APPLY,
            ACTION,
            REJECT,
            NO,
            DESTRUCTIVE,
            ACCEPT,
            YES,
        ],
        _ => [
            HELP,
            RESET,
            ACTION,
            APPLY,
            DESTRUCTIVE,
            REJECT,
            ACCEPT,
            NO,
            YES,
        ],
    }
}

fn rank(button_layout: i32, role: i32) -> usize {
    let order = role_order(button_layout);
    order.iter().position(|r| *r == role).unwrap_or(order.len())
}

/// The order the platform the program was built for puts buttons in.
/// Qt asks the desktop's theme at run time; every Linux desktop the
/// oracle has run on answered GNOME's, and so does this.
fn platform_layout() -> i32 {
    if cfg!(target_os = "windows") {
        layout::WIN
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        layout::MAC
    } else if cfg!(target_os = "android") {
        layout::ANDROID
    } else {
        layout::GNOME
    }
}

// ------------------------------------------------------- DialogButtonBox

/// A button of a box and what the box put on it.
#[derive(Clone, Debug)]
struct Member {
    button: ObjectId,
    /// The box listens to the button's `clicked`.
    clicked: ConnectionId,
    /// Where the box holds the button's position.
    x: OverrideId,
    y: OverrideId,
    /// And its size, along each axis its author did not give.
    width: Option<OverrideId>,
    height: Option<OverrideId>,
    /// The flag of the standard button this is, 0 for one the author made.
    standard: i32,
}

/// What a box remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct BoxState {
    /// The buttons in the order they came.
    members: RefCell<Vec<Member>>,
    /// The same buttons in the order of the row.
    row: RefCell<Vec<ObjectId>>,
}

/// `DialogButtonBox`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "DialogButtonBox",
    extends = Control,
    default = "contentData",
    enums = box_enums,
    attached = DialogButtonBoxAttached
)]
pub struct DialogButtonBox {
    #[qml(base)]
    pub control: Control,
    #[qml(notify, list, append = box_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
    /// How many buttons the box has.
    #[qml(notify, readonly)]
    pub count: i32,
    /// `Qt.Alignment` flags: where the buttons are in a box that is
    /// larger than they need. 0 lets them share the whole width.
    #[qml(notify, flags = "Qt.Alignment")]
    pub alignment: i32,
    #[qml(notify, enumeration = "ButtonLayout")]
    pub button_layout: i32,
    /// What a standard button is made from.
    #[qml(notify)]
    pub delegate: Option<ObjectId>,
    /// Whether the box is the header or the footer of what it is in, for
    /// a style that draws the two differently.
    #[qml(notify, enumeration = "Position")]
    pub position: i32,
    #[qml(notify, flags = "StandardButton")]
    pub standard_buttons: i32,
    #[qml(notify, reset = BUTTONS_ASKED)]
    pub content_width: f64,
    #[qml(notify, reset = BUTTONS_ASKED)]
    pub content_height: f64,
    #[qml(skip)]
    pub(crate) state: Rc<BoxState>,
}

/// `contentWidth = undefined`: the box has what its buttons ask for again.
/// Qt's keeps the size it was given until its buttons next change; here it
/// follows at once
/// (`features/bindings/property-reset-content-size-at-once.feature`).
const BUTTONS_ASKED: ljos_engine::Reset = ljos_engine::Reset::Follow(buttons_asked);

fn buttons_asked(engine: &Engine, bx: ObjectId, size: ljos_engine::PropIndex) -> Value {
    let wide = size == DialogButtonBox::CONTENT_WIDTH.index;
    Value::Real(asked(engine, bx).map_or(0.0, |a| if wide { a.implicit.0 } else { a.implicit.1 }))
}

impl Default for DialogButtonBox {
    fn default() -> Self {
        Self {
            control: Control::default(),
            content_data: QmlList::default(),
            content_children: QmlList::default(),
            count: 0,
            alignment: 0,
            button_layout: platform_layout(),
            delegate: None,
            position: 1,
            standard_buttons: 0,
            content_width: 0.0,
            content_height: 0.0,
            state: Rc::default(),
        }
    }
}

/// What `DialogButtonBox.buttonRole: …` attaches to a button.
#[derive(Debug, QmlObject)]
#[qml(name = "DialogButtonBoxAttached", enums = box_enums)]
pub struct DialogButtonBoxAttached {
    /// The box the button is in.
    #[qml(notify, readonly)]
    pub button_box: Option<ObjectId>,
    #[qml(notify, enumeration = "ButtonRole")]
    pub button_role: i32,
}

impl Default for DialogButtonBoxAttached {
    fn default() -> Self {
        Self {
            button_box: None,
            button_role: role::INVALID,
        }
    }
}

const BOX: Host = Host {
    content_item: Control::CONTENT_ITEM.index,
    content_data: DialogButtonBox::CONTENT_DATA.index,
    content_children: DialogButtonBox::CONTENT_CHILDREN.index,
};

fn box_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    host_append(cx, child, BOX)?;
    let e = cx.engine;
    // What is declared in the box is taken in when the box is complete,
    // when the roles of its buttons are known.
    let complete = e
        .flags(cx.this)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if complete && is_a::<AbstractButton>(e, child) {
        join(e, cx.this, child, None, 0)?;
        sort(e, cx.this)?;
    }
    Ok(())
}

#[qml_impl]
impl DialogButtonBox {
    /// A button of the box was clicked.
    #[qml(signal)]
    fn clicked(button: Option<ObjectId>);
    /// A button that accepts (`AcceptRole`, `YesRole`) was clicked.
    #[qml(signal)]
    fn accepted();
    #[qml(signal)]
    fn rejected();
    #[qml(signal)]
    fn applied();
    #[qml(signal)]
    fn discarded();
    #[qml(signal, name = "helpRequested")]
    fn help_requested();
    #[qml(signal)]
    fn reset();

    /// The button at `index` of the row.
    #[qml(invokable, name = "itemAt")]
    fn item_at(cx: &Cx, index: Value) -> Value {
        usize::try_from(index.to_number() as i64)
            .ok()
            .and_then(|i| row(cx.engine, cx.this).get(i).copied())
            .map_or(Value::Null, Value::Object)
    }

    #[qml(invokable, name = "addItem")]
    fn add_item(cx: &Cx, item: Option<ObjectId>) -> Result<(), EngineError> {
        insert(cx.engine, cx.this, item, None)
    }

    /// The role of the button decides where in the row it goes; `index`
    /// is its place among the buttons of the same role.
    #[qml(invokable, name = "insertItem")]
    fn insert_item(cx: &Cx, index: Value, item: Option<ObjectId>) -> Result<(), EngineError> {
        let at = usize::try_from(index.to_number() as i64).ok();
        insert(cx.engine, cx.this, item, at)
    }

    /// Takes a button out of the box and destroys it.
    #[qml(invokable, name = "removeItem")]
    fn remove_item(cx: &Cx, item: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        // Qt 6 is given the item; Qt 5 was given its index.
        let button = match item {
            Value::Object(o) => Some(o),
            Value::Undefined | Value::Null => None,
            index => usize::try_from(index.to_number() as i64)
                .ok()
                .and_then(|i| row(e, cx.this).get(i).copied()),
        };
        if let Some(b) = button
            && leave(e, cx.this, b)?
        {
            sort(e, cx.this)?;
            e.destroy(b);
        }
        Ok(())
    }

    /// Takes the button at `index` out of the box and returns it.
    #[qml(invokable, name = "takeItem")]
    fn take_item(cx: &Cx, index: Value) -> Result<Value, EngineError> {
        let e = cx.engine;
        let button = usize::try_from(index.to_number() as i64)
            .ok()
            .and_then(|i| row(e, cx.this).get(i).copied());
        match button {
            Some(b) if leave(e, cx.this, b)? => {
                sort(e, cx.this)?;
                Ok(Value::Object(b))
            }
            _ => Ok(Value::Null),
        }
    }

    /// The button the box made for a standard button flag.
    #[qml(invokable, name = "standardButton")]
    fn standard_button(cx: &Cx, button: Value) -> Value {
        standard_button(cx.engine, cx.this, button.to_number() as i32)
            .map_or(Value::Null, Value::Object)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_box(cx.engine, cx.this)
    }
}

fn box_state(engine: &Engine, bx: ObjectId) -> Option<Rc<BoxState>> {
    engine
        .with::<DialogButtonBox, _>(bx, |b| b.state.clone())
        .ok()
}

/// The buttons of a box in the order of its row.
fn row(engine: &Engine, bx: ObjectId) -> Vec<ObjectId> {
    box_state(engine, bx).map_or_else(Vec::new, |s| {
        s.row
            .borrow()
            .iter()
            .copied()
            .filter(|b| engine.is_alive(*b))
            .collect()
    })
}

fn attacher() -> MetaRef {
    MetaRef::Static(DialogButtonBox::static_meta())
}

fn role_of(engine: &Engine, button: ObjectId) -> i32 {
    engine
        .existing_attached_object(button, &attacher())
        .map_or(role::INVALID, |att| {
            engine
                .untrack(|| engine.read(att, DialogButtonBoxAttached::BUTTON_ROLE.index))
                .to_number() as i32
        })
}

fn set_box(engine: &Engine, button: ObjectId, bx: Option<ObjectId>) -> Result<(), EngineError> {
    let att = engine.attached_object(button, &attacher())?;
    set_computed(
        engine,
        att,
        DialogButtonBoxAttached::BUTTON_BOX.index,
        bx.map_or(Value::Null, Value::Object),
    )
}

fn init_box(e: &Engine, bx: ObjectId) -> Result<(), EngineError> {
    host_complete(e, bx, BOX)?;
    let declared: Vec<ObjectId> = match e.untrack(|| e.read(bx, BOX.content_children)) {
        Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    };
    for child in declared {
        if e.is_alive(child) && is_a::<AbstractButton>(e, child) {
            join(e, bx, child, None, 0)?;
        }
    }
    make_standard(e, bx)?;
    sort(e, bx)?;

    e.bind_initial(bx, Control::IMPLICIT_CONTENT_WIDTH.index, move |e| {
        Value::Real(asked(e, bx).map_or(0.0, |a| a.implicit.0))
    })?;
    e.bind_initial(bx, Control::IMPLICIT_CONTENT_HEIGHT.index, move |e| {
        Value::Real(asked(e, bx).map_or(0.0, |a| a.implicit.1))
    })?;
    bind_default(e, bx, DialogButtonBox::CONTENT_WIDTH.index, move |e| {
        Value::Real(asked(e, bx).map_or(0.0, |a| a.implicit.0))
    })?;
    bind_default(e, bx, DialogButtonBox::CONTENT_HEIGHT.index, move |e| {
        Value::Real(asked(e, bx).map_or(0.0, |a| a.implicit.1))
    })?;

    e.watch(bx, DialogButtonBox::BUTTON_LAYOUT.index, move |e, _| {
        check(e, "buttonLayout", sort(e, bx));
    })?;
    for prop in [
        DialogButtonBox::STANDARD_BUTTONS.index,
        DialogButtonBox::DELEGATE.index,
    ] {
        e.watch(bx, prop, move |e, _| {
            let made = make_standard(e, bx).and_then(|()| sort(e, bx));
            check(e, "standardButtons", made);
        })?;
    }
    e.effect_in(bx, move |e| {
        let Some(m) = measure(e, bx) else {
            return;
        };
        e.untrack(|| check(e, "buttons", place(e, bx, &m)));
    })?;
    Ok(())
}

/// Takes `button` into the box, as the `at`-th of its buttons or the
/// last. The row is not put in order yet.
fn join(
    e: &Engine,
    bx: ObjectId,
    button: ObjectId,
    at: Option<usize>,
    standard: i32,
) -> Result<(), EngineError> {
    let Some(st) = box_state(e, bx) else {
        return Ok(());
    };
    if let Some(m) = st
        .members
        .borrow_mut()
        .iter_mut()
        .find(|m| m.button == button)
    {
        // Making a button in the box has put it in already, as one the
        // author might have declared.
        if standard != 0 {
            m.standard = standard;
        }
        return Ok(());
    }
    // A size the author gave the button is the button's.
    let (own_width, own_height) = e.untrack(|| {
        (
            !follows_implicit_size(e, button, true),
            !follows_implicit_size(e, button, false),
        )
    });
    let hold = |prop: PropIndex| {
        let now = e.untrack(|| e.read(button, prop));
        e.push_override(button, prop, OverrideContent::Value(now))
    };
    let member = Member {
        button,
        clicked: e.connect_by_name(button, "clicked", move |e, _| clicked(e, bx, button))?,
        x: hold(Item::X.index)?,
        y: hold(Item::Y.index)?,
        width: (!own_width).then(|| hold(Item::WIDTH.index)).transpose()?,
        height: (!own_height)
            .then(|| hold(Item::HEIGHT.index))
            .transpose()?,
        standard,
    };
    {
        let mut members = st.members.borrow_mut();
        let at = at.map_or(members.len(), |i| i.min(members.len()));
        members.insert(at, member);
    }
    set_box(e, button, Some(bx))
}

/// Takes `button` out of the box: it is no child of it any more and
/// keeps what it was given. False if it was not in the box.
fn leave(e: &Engine, bx: ObjectId, button: ObjectId) -> Result<bool, EngineError> {
    let Some(st) = box_state(e, bx) else {
        return Ok(false);
    };
    let member = {
        let mut members = st.members.borrow_mut();
        match members.iter().position(|m| m.button == button) {
            Some(i) => members.remove(i),
            None => return Ok(false),
        }
    };
    e.disconnect(member.clicked);
    if !e.is_alive(button) {
        return Ok(true);
    }
    for (prop, layer) in [
        (Item::X.index, Some(member.x)),
        (Item::Y.index, Some(member.y)),
        (Item::WIDTH.index, member.width),
        (Item::HEIGHT.index, member.height),
    ] {
        if let Some(layer) = layer {
            let _ = e.remove_override(button, prop, layer, RestoreMode::BindingOrValue);
        }
    }
    e.list_remove(bx, BOX.content_data, button)?;
    e.list_remove(bx, BOX.content_children, button)?;
    set_parent_item(e, button, None)?;
    set_box(e, button, None)?;
    Ok(true)
}

/// Makes `item` a child of the box, if it is not one.
fn adopt(e: &Engine, bx: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    let known = matches!(
        e.untrack(|| e.read(bx, BOX.content_data)),
        Value::List(l) if l.iter().any(|v| v.as_object() == Some(item))
    );
    if !known {
        e.list_push(bx, BOX.content_data, item)?;
        if ljos_quick::is_item(e, item) {
            e.list_push(bx, BOX.content_children, item)?;
        }
    }
    if let Some(ci) = e.untrack(|| object(e, bx, BOX.content_item))
        && ljos_quick::is_item(e, item)
    {
        set_parent_item(e, item, Some(ci))?;
    }
    Ok(())
}

/// `addItem` and `insertItem`.
fn insert(
    e: &Engine,
    bx: ObjectId,
    item: Option<ObjectId>,
    at: Option<usize>,
) -> Result<(), EngineError> {
    let Some(item) = item.filter(|i| e.is_alive(*i)) else {
        return Ok(());
    };
    adopt(e, bx, item)?;
    if is_a::<AbstractButton>(e, item) {
        join(e, bx, item, at, 0)?;
        sort(e, bx)?;
    }
    Ok(())
}

/// Puts the row in the order of the platform: by role, and among the
/// buttons of one role in the order they came. A role that changes later
/// moves nothing until then, as in Qt.
fn sort(e: &Engine, bx: ObjectId) -> Result<(), EngineError> {
    let Some(st) = box_state(e, bx) else {
        return Ok(());
    };
    st.members.borrow_mut().retain(|m| e.is_alive(m.button));
    let button_layout = e
        .untrack(|| e.read(bx, DialogButtonBox::BUTTON_LAYOUT.index))
        .to_number() as i32;
    let mut row: Vec<ObjectId> = st.members.borrow().iter().map(|m| m.button).collect();
    row.sort_by_key(|b| rank(button_layout, role_of(e, *b)));
    let count = row.len() as i32;
    let moved = *st.row.borrow() != row;
    *st.row.borrow_mut() = row;
    let p = DialogButtonBox::COUNT.index;
    if e.untrack(|| e.read(bx, p)).to_number() as i32 != count {
        e.write_internal(bx, p, Value::Int(count), WriteKind::Imperative)
    } else if moved {
        // Nothing the row's size is worked out from has changed, but
        // where each button is has.
        match e.untrack(|| measure(e, bx)) {
            Some(m) => e.untrack(|| place(e, bx, &m)),
            None => Ok(()),
        }
    } else {
        Ok(())
    }
}

/// Makes the buttons `standardButtons` asks for, in place of the ones
/// made before.
fn make_standard(e: &Engine, bx: ObjectId) -> Result<(), EngineError> {
    let Some(st) = box_state(e, bx) else {
        return Ok(());
    };
    let old: Vec<ObjectId> = st
        .members
        .borrow()
        .iter()
        .filter(|m| m.standard != 0)
        .map(|m| m.button)
        .collect();
    for b in old {
        leave(e, bx, b)?;
        e.destroy(b);
    }
    let wanted = e
        .untrack(|| e.read(bx, DialogButtonBox::STANDARD_BUTTONS.index))
        .to_number() as i32;
    if wanted == 0 {
        return Ok(());
    }
    let delegate = e.untrack(|| object(e, bx, DialogButtonBox::DELEGATE.index));
    for (flag, _, text, role) in STANDARD {
        if wanted & flag == 0 {
            continue;
        }
        let button = match delegate {
            Some(d) => e.component_create(d, Some(bx), Vec::new())?,
            None => {
                // A box without a style: the template, which draws nothing.
                let b = e.create_native(Button::default(), Some(bx))?;
                e.complete(b)?;
                b
            }
        };
        if !is_a::<AbstractButton>(e, button) {
            crate::util::warn(e, "DialogButtonBox: the delegate is not a button");
            e.destroy(button);
            continue;
        }
        // A binding, so that the text follows the language.
        e.bind(button, AbstractButton::TEXT.index, move |e| {
            Value::String(Atom::from(e.translate(TEXT_CONTEXT, text, "", None)))
        })?;
        let att = e.attached_object(button, &attacher())?;
        e.write(
            att,
            DialogButtonBoxAttached::BUTTON_ROLE.index,
            Value::Int(role),
            WriteKind::Imperative,
        )?;
        adopt(e, bx, button)?;
        join(e, bx, button, None, flag)?;
    }
    Ok(())
}

fn standard_button(engine: &Engine, bx: ObjectId, flag: i32) -> Option<ObjectId> {
    let st = box_state(engine, bx)?;
    let members = st.members.borrow();
    members
        .iter()
        .find(|m| m.standard != 0 && m.standard == flag && engine.is_alive(m.button))
        .map(|m| m.button)
}

/// A button of the box said `clicked`: the box says which, and then what
/// the button's role means.
fn clicked(e: &Engine, bx: ObjectId, button: ObjectId) {
    if !e.is_alive(bx) {
        return;
    }
    let role = role_of(e, button);
    check(
        e,
        "clicked",
        e.emit_by_name(bx, "clicked", &[Value::Object(button)]),
    );
    let signal = match role {
        role::ACCEPT | role::YES => "accepted",
        role::REJECT | role::NO => "rejected",
        role::APPLY => "applied",
        role::RESET => "reset",
        role::DESTRUCTIVE => "discarded",
        role::HELP => "helpRequested",
        _ => return,
    };
    if e.is_alive(bx) {
        check(e, signal, e.emit_by_name(bx, signal, &[]));
    }
}

// ----------------------------------------------------------- the row

const ALIGN_LEFT: i32 = 0x01;
const ALIGN_RIGHT: i32 = 0x02;
const ALIGN_H_CENTER: i32 = 0x04;
const ALIGN_TOP: i32 = 0x20;
const ALIGN_BOTTOM: i32 = 0x40;

/// What the buttons of a box ask for.
#[derive(Debug, Default)]
struct Asked {
    /// The buttons in the order of the row, and the size each asks for.
    buttons: Vec<(Member, f64, f64)>,
    spacing: f64,
    alignment: i32,
    /// The tallest button.
    tallest: f64,
    /// What the row asks for.
    implicit: (f64, f64),
}

impl Asked {
    fn sideways(&self) -> bool {
        self.alignment & (ALIGN_LEFT | ALIGN_RIGHT | ALIGN_H_CENTER) != 0
    }

    fn gaps(&self) -> f64 {
        self.spacing * (self.buttons.len() as f64 - 1.0).max(0.0)
    }
}

/// Where a box's content item and its buttons are.
#[derive(Debug, Default, PartialEq)]
struct Measured {
    /// The content item: x, y, width, height.
    rect: [f64; 4],
    /// Each button of the row: x, width, height.
    cells: Vec<(ObjectId, [f64; 3])>,
}

/// What the row asks for (tracked). It reads nothing of the size the box
/// has: the box's size comes from this.
fn asked(e: &Engine, bx: ObjectId) -> Option<Asked> {
    let st = box_state(e, bx)?;
    // The row changes with the count; the order with the layout.
    let _ = e.read(bx, DialogButtonBox::COUNT.index);
    let _ = e.read(bx, DialogButtonBox::BUTTON_LAYOUT.index);
    let row: Vec<Member> = {
        let members = st.members.borrow();
        st.row
            .borrow()
            .iter()
            .filter_map(|b| members.iter().find(|m| m.button == *b).cloned())
            .filter(|m| e.is_alive(m.button))
            .collect()
    };
    let buttons: Vec<(Member, f64, f64)> = row
        .into_iter()
        .map(|m| {
            let w = num(e, m.button, Item::IMPLICIT_WIDTH.index);
            let h = num(e, m.button, Item::IMPLICIT_HEIGHT.index);
            (m, w, h)
        })
        .collect();
    let mut a = Asked {
        spacing: num(e, bx, Control::SPACING.index),
        alignment: e.read(bx, DialogButtonBox::ALIGNMENT.index).to_number() as i32,
        tallest: buttons.iter().fold(0.0_f64, |a, b| a.max(b.2)),
        buttons,
        implicit: (0.0, 0.0),
    };
    let n = a.buttons.len() as f64;
    let width = if a.sideways() {
        a.buttons.iter().map(|b| b.1).sum::<f64>()
    } else {
        // Each as wide as the widest.
        a.buttons.iter().fold(0.0_f64, |w, b| w.max(b.1)) * n
    };
    a.implicit = (width + a.gaps(), a.tallest);
    Some(a)
}

/// Works the row out (tracked). Without an alignment the buttons share
/// the whole width and are each as wide as the others; with one they are
/// as wide as they ask and sit together where the alignment says.
fn measure(e: &Engine, bx: ObjectId) -> Option<Measured> {
    let a = asked(e, bx)?;
    let n = a.buttons.len();
    let (left, top) = (
        num(e, bx, Control::LEFT_PADDING.index),
        num(e, bx, Control::TOP_PADDING.index),
    );
    let (room_w, room_h) = (
        num(e, bx, Control::AVAILABLE_WIDTH.index),
        num(e, bx, Control::AVAILABLE_HEIGHT.index),
    );
    if n == 0 {
        return Some(Measured {
            rect: [left, top, room_w, room_h],
            cells: Vec::new(),
        });
    }
    let (sideways, align, tallest) = (a.sideways(), a.alignment, a.tallest);
    let share = ((room_w - a.gaps()) / n as f64).max(0.0);
    let mut x = 0.0;
    let mut cells = Vec::with_capacity(n);
    for (m, iw, _) in &a.buttons {
        let w = if m.width.is_none() {
            num(e, m.button, Item::WIDTH.index)
        } else if !sideways {
            share
        } else if n == 1 && m.standard != 0 {
            // The only standard button of a box takes half of it.
            room_w / 2.0
        } else {
            *iw
        };
        let h = if m.height.is_none() {
            num(e, m.button, Item::HEIGHT.index)
        } else {
            tallest
        };
        cells.push((m.button, [x, w, h]));
        x += w + a.spacing;
    }
    let taken = x - a.spacing;
    let (rx, rw) = if !sideways {
        (left, room_w)
    } else if align & ALIGN_RIGHT != 0 {
        (left + room_w - taken, taken)
    } else if align & ALIGN_H_CENTER != 0 {
        (left + (room_w - taken) / 2.0, taken)
    } else {
        (left, taken)
    };
    let (ry, rh) = if align == 0 {
        (top, room_h)
    } else if align & ALIGN_TOP != 0 {
        (top, tallest)
    } else if align & ALIGN_BOTTOM != 0 {
        (top + room_h - tallest, tallest)
    } else {
        (top + (room_h - tallest) / 2.0, tallest)
    };
    Some(Measured {
        rect: [rx, ry, rw, rh],
        cells,
    })
}

/// Puts the buttons where the row has them.
fn place(e: &Engine, bx: ObjectId, m: &Measured) -> Result<(), EngineError> {
    let Some(st) = box_state(e, bx) else {
        return Ok(());
    };
    let members = st.members.borrow().clone();
    for (button, [x, w, h]) in &m.cells {
        let Some(member) = members.iter().find(|mb| mb.button == *button) else {
            continue;
        };
        for (prop, layer, v) in [
            (Item::X.index, Some(member.x), *x),
            (Item::Y.index, Some(member.y), 0.0),
            (Item::WIDTH.index, member.width, *w),
            (Item::HEIGHT.index, member.height, *h),
        ] {
            let Some(layer) = layer else {
                continue;
            };
            if e.read(*button, prop).to_number() != v {
                e.write_internal(*button, prop, Value::Real(v), WriteKind::Override(layer))?;
            }
        }
    }
    Ok(())
}

/// Where the content item of a button box is: around its buttons where
/// the box's alignment puts them (tracked). None for any other control.
pub(crate) fn button_box_rect(engine: &Engine, obj: ObjectId) -> Option<[f64; 4]> {
    if !is_a::<DialogButtonBox>(engine, obj) {
        return None;
    }
    measure(engine, obj).map(|m| m.rect)
}

// ---------------------------------------------------------------- Dialog

/// `Dialog`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Dialog",
    extends = Popup,
    enums = dialog_enums
)]
pub struct Dialog {
    #[qml(base)]
    pub popup: Popup,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub header: Option<ObjectId>,
    #[qml(notify)]
    pub footer: Option<ObjectId>,
    #[qml(notify, flags = "StandardButton")]
    pub standard_buttons: i32,
    /// `Dialog.Accepted`, `Dialog.Rejected` or what `done()` was given.
    #[qml(notify)]
    pub result: i32,
    #[qml(notify, readonly)]
    pub implicit_header_width: f64,
    #[qml(notify, readonly)]
    pub implicit_header_height: f64,
    #[qml(notify, readonly)]
    pub implicit_footer_width: f64,
    #[qml(notify, readonly)]
    pub implicit_footer_height: f64,
    /// What the dialog listens to on its footer.
    #[qml(skip)]
    wiring: Rc<RefCell<Vec<ConnectionId>>>,
}

impl Default for Dialog {
    fn default() -> Self {
        // A dialog takes the keyboard when it opens.
        let popup = Popup {
            focus: true,
            ..Popup::default()
        };
        Self {
            popup,
            title: Atom::EMPTY,
            header: None,
            footer: None,
            standard_buttons: 0,
            result: 0,
            implicit_header_width: 0.0,
            implicit_header_height: 0.0,
            implicit_footer_width: 0.0,
            implicit_footer_height: 0.0,
            wiring: Rc::default(),
        }
    }
}

#[qml_impl]
impl Dialog {
    #[qml(signal)]
    fn accepted();
    #[qml(signal)]
    fn rejected();
    /// A button with `ApplyRole` was clicked.
    #[qml(signal)]
    fn applied();
    #[qml(signal)]
    fn discarded();
    #[qml(signal, name = "helpRequested")]
    fn help_requested();
    #[qml(signal)]
    fn reset();

    #[qml(invokable)]
    fn accept(cx: &Cx) -> Result<(), EngineError> {
        done(cx.engine, cx.this, 1)
    }

    #[qml(invokable)]
    fn reject(cx: &Cx) -> Result<(), EngineError> {
        reject(cx.engine, cx.this)
    }

    #[qml(invokable)]
    fn done(cx: &Cx, result: Value) -> Result<(), EngineError> {
        done(cx.engine, cx.this, result.to_number() as i32)
    }

    /// The button the footer made for a standard button flag.
    #[qml(invokable, name = "standardButton")]
    fn standard_button(cx: &Cx, button: Value) -> Value {
        let e = cx.engine;
        e.untrack(|| object(e, cx.this, Dialog::FOOTER.index))
            .filter(|f| is_a::<DialogButtonBox>(e, *f))
            .and_then(|f| standard_button(e, f, button.to_number() as i32))
            .map_or(Value::Null, Value::Object)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_dialog(cx.engine, cx.this)
    }
}

fn emit(engine: &Engine, obj: ObjectId, signal: &str) {
    if engine.is_alive(obj) {
        check(engine, signal, engine.emit_by_name(obj, signal, &[]));
    }
}

/// Closes the dialog with `result`. A dialog that is closed already still
/// gets the result and says so.
pub(crate) fn done(e: &Engine, dialog: ObjectId, result: i32) -> Result<(), EngineError> {
    popup::put(e, dialog, Dialog::RESULT.index, Value::Int(result))?;
    match result {
        1 => emit(e, dialog, "accepted"),
        0 => emit(e, dialog, "rejected"),
        _ => {}
    }
    if e.is_alive(dialog) {
        popup::close(e, dialog)?;
    }
    Ok(())
}

/// What Escape and a press beside the dialog do as well.
pub(crate) fn reject(e: &Engine, dialog: ObjectId) -> Result<(), EngineError> {
    done(e, dialog, 0)
}

fn init_dialog(e: &Engine, dialog: ObjectId) -> Result<(), EngineError> {
    let Some(item) = popup::item_of(e, dialog) else {
        return Ok(());
    };
    // The popup's item is a page: it places the header and the footer.
    for (to, from) in [
        (Page::TITLE.index, Dialog::TITLE.index),
        (Page::HEADER.index, Dialog::HEADER.index),
        (Page::FOOTER.index, Dialog::FOOTER.index),
    ] {
        popup::follow(e, item, to, dialog, from)?;
    }
    for (out, from) in [
        (
            Dialog::IMPLICIT_HEADER_WIDTH.index,
            Page::IMPLICIT_HEADER_WIDTH.index,
        ),
        (
            Dialog::IMPLICIT_HEADER_HEIGHT.index,
            Page::IMPLICIT_HEADER_HEIGHT.index,
        ),
        (
            Dialog::IMPLICIT_FOOTER_WIDTH.index,
            Page::IMPLICIT_FOOTER_WIDTH.index,
        ),
        (
            Dialog::IMPLICIT_FOOTER_HEIGHT.index,
            Page::IMPLICIT_FOOTER_HEIGHT.index,
        ),
    ] {
        e.bind_initial(dialog, out, move |e| e.read(item, from))?;
    }
    let footer = e.untrack(|| object(e, dialog, Dialog::FOOTER.index));
    wire(e, dialog, footer)?;
    e.watch(dialog, Dialog::FOOTER.index, move |e, v| {
        let footer = v.as_object().filter(|f| e.is_alive(*f));
        check(e, "footer", wire(e, dialog, footer));
    })?;
    e.watch(dialog, Dialog::STANDARD_BUTTONS.index, move |e, v| {
        let footer = e.untrack(|| object(e, dialog, Dialog::FOOTER.index));
        if let Some(f) = footer.filter(|f| is_a::<DialogButtonBox>(e, *f)) {
            let p = DialogButtonBox::STANDARD_BUTTONS.index;
            check(e, "standardButtons", popup::put(e, f, p, v.clone()));
        }
    })?;
    Ok(())
}

/// Makes the dialog do what the buttons of its footer ask, when the
/// footer is a button box, and hands the box the dialog's standard
/// buttons.
fn wire(e: &Engine, dialog: ObjectId, footer: Option<ObjectId>) -> Result<(), EngineError> {
    let Ok(wiring) = e.with::<Dialog, _>(dialog, |d| d.wiring.clone()) else {
        return Ok(());
    };
    for conn in wiring.borrow_mut().drain(..) {
        e.disconnect(conn);
    }
    let Some(bx) = footer.filter(|f| is_a::<DialogButtonBox>(e, *f)) else {
        return Ok(());
    };
    let wanted = e.untrack(|| e.read(dialog, Dialog::STANDARD_BUTTONS.index));
    if wanted.to_number() != 0.0 {
        popup::put(e, bx, DialogButtonBox::STANDARD_BUTTONS.index, wanted)?;
    }
    let mut made = Vec::new();
    made.push(e.connect_by_name(bx, "accepted", move |e, _| {
        check(e, "accept", done(e, dialog, 1));
    })?);
    made.push(e.connect_by_name(bx, "rejected", move |e, _| {
        check(e, "reject", reject(e, dialog));
    })?);
    for signal in ["applied", "reset", "helpRequested"] {
        made.push(e.connect_by_name(bx, signal, move |e, _| emit(e, dialog, signal))?);
    }
    // Discarding is neither: the dialog closes and its result stays.
    made.push(e.connect_by_name(bx, "discarded", move |e, _| {
        emit(e, dialog, "discarded");
        if e.is_alive(dialog) {
            check(e, "close", popup::close(e, dialog));
        }
    })?);
    *wiring.borrow_mut() = made;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{layout, rank, role};

    #[test]
    fn a_role_no_layout_knows_comes_last() {
        for l in [
            layout::WIN,
            layout::MAC,
            layout::KDE,
            layout::GNOME,
            layout::ANDROID,
        ] {
            assert_eq!(rank(l, role::INVALID), 9);
            assert_eq!(rank(l, 42), 9);
        }
    }

    #[test]
    fn accepting_is_before_rejecting_only_where_the_platform_has_it_so() {
        let before = |l: i32| rank(l, role::ACCEPT) < rank(l, role::REJECT);
        assert!(before(layout::WIN));
        assert!(!before(layout::MAC));
        assert!(!before(layout::GNOME));
        assert!(before(layout::KDE));
        assert!(!before(layout::ANDROID));
    }
}
