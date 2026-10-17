//! `Popup` (QtQuick.Templates): something shown above the content of its
//! window.
//!
//! A popup is not an item. What draws is an item this module makes for
//! it, the *popup item*, which lives in the window's overlay
//! ([`overlay`]) and not under the item that declared the popup: nothing
//! in the content clips or covers it, and the positioner the popup was
//! declared in does not see it. The popup's properties are handed to that
//! item, which is a `Page` underneath and so places the background, the
//! content item and (for a `Dialog`) a header and a footer.
//!
//! A closed popup's item stays in the overlay, hidden. What is inside a
//! closed popup therefore reads `visible: false` and costs nothing.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-popup.html and
//! doc.qt.io/qt-6/qtquickcontrols-popups.html; pinned by
//! `features/controls/popup*.feature` and `overlay.feature`.
//!
//! A popup with an `enter` transition is `opened` when that has run, and
//! one with an `exit` transition stays visible until that has run and is
//! `closed` then (measured on Qt 6.11.2, pinned by
//! `popup-transitions.feature`). The transitions run on the popup, whose
//! `opacity` and `scale` are its item's. With reduced motion they are
//! over in the step they start in.

mod drawer;
mod drawer_drag;
mod item;
mod overlay;
mod position;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, Font, Interception, Interceptor, ObjectId, Prop, PropIndex, QmlList,
    QmlObject, QtObject, Value, WriteKind, qml_impl,
};
use ljos_quick::focus::{FocusReason, active_focus_item, force_active_focus};
use ljos_quick::{Item, is_item, set_parent_item};

use crate::containers::{Host, content_size, host_append, host_complete};
use crate::control::Control;
use crate::palette::Palette;
use crate::parts::settle_parts;
use crate::util::{bind_default, check, flag, is_a, num, object, set_computed};
use crate::window::Window;

pub use drawer::Drawer;
pub(crate) use item::{PopupItem, follow, look_from_above, set_role, take_look_back};
pub(crate) use overlay::last_pointer;
pub use overlay::{Overlay, OverlayAttached};

/// `Popup.closePolicy` flags.
pub(crate) mod close_policy {
    pub const ON_PRESS_OUTSIDE: i32 = 1;
    pub const ON_PRESS_OUTSIDE_PARENT: i32 = 2;
    pub const ON_RELEASE_OUTSIDE: i32 = 4;
    pub const ON_RELEASE_OUTSIDE_PARENT: i32 = 8;
    pub const ON_ESCAPE: i32 = 16;
}

fn popup_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ClosePolicy",
            vec![
                ("NoAutoClose", 0),
                ("CloseOnPressOutside", 1),
                ("CloseOnPressOutsideParent", 2),
                ("CloseOnReleaseOutside", 4),
                ("CloseOnReleaseOutsideParent", 8),
                ("CloseOnEscape", 16),
            ],
        ),
        ("PopupType", vec![("Item", 0), ("Window", 1), ("Native", 2)]),
        (
            "TransformOrigin",
            vec![
                ("TopLeft", 0),
                ("Top", 1),
                ("TopRight", 2),
                ("Left", 3),
                ("Center", 4),
                ("Right", 5),
                ("BottomLeft", 6),
                ("Bottom", 7),
                ("BottomRight", 8),
            ],
        ),
    ]
}

/// The `anchors` group of a popup: it can only be centred.
#[derive(Clone, Debug, Default, QmlObject)]
#[qml(name = "PopupAnchors", uncreatable)]
pub struct PopupAnchors {
    #[qml(notify, reset)]
    pub center_in: Option<ObjectId>,
}

/// What a popup remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct PopupState {
    /// The item that draws the popup.
    pub item: Cell<Option<ObjectId>>,
    /// The shade behind a modal or dimming popup, while it is shown.
    pub dimmer: Cell<Option<ObjectId>>,
    /// Shown in an overlay: open, and its window is known.
    pub shown: Cell<bool>,
    /// What had the keyboard before the popup took it.
    saved_focus: Cell<Option<ObjectId>>,
    took_focus: Cell<bool>,
    /// `open()` and `close()` write `visible` themselves.
    switching: Cell<bool>,
    /// Which of the popup's transitions is running.
    phase: Cell<Phase>,
    /// The opacity and scale the popup had when it began to go: it has
    /// them again when it has gone, so that it can be shown again.
    before_exit: Cell<Option<(f64, f64)>>,
    /// The position code is writing `x` and `y` back.
    pub placing: Cell<bool>,
    /// Where the author put the popup; `x` and `y` read as where it is.
    pub requested: Cell<(f64, f64)>,
    /// What the position code last wrote back to `x` and `y`.
    pub placed: Cell<Option<(f64, f64)>>,
    /// The press that is down began outside the popup.
    pub pressed_outside: Cell<bool>,
    /// What the control the popup belongs to does each time the popup is
    /// shown, once it has its size and its place.
    when_shown: WhenShown,
}

/// Where a popup is between shown and hidden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// Open or closed: no transition runs.
    #[default]
    Rest,
    /// Its enter transition runs: visible, not yet opened.
    Coming,
    /// Its exit transition runs: still visible, no longer opened.
    Going,
}

/// What `when_shown` has called.
pub(crate) type Shown = Rc<dyn Fn(&Engine)>;

#[derive(Default)]
struct WhenShown(RefCell<Option<Shown>>);

impl std::fmt::Debug for WhenShown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.borrow().is_some() {
            "WhenShown(set)"
        } else {
            "WhenShown(none)"
        })
    }
}

/// Has `f` called each time `popup` is shown, after it was sized and
/// placed and before it says `opened`; `None` takes that away. A combo
/// box brings its current row into view there.
pub(crate) fn when_shown(engine: &Engine, popup: ObjectId, f: Option<Shown>) {
    if let Some(state) = state_of(engine, popup) {
        *state.when_shown.0.borrow_mut() = f;
    }
}

/// `Popup`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Popup",
    extends = QtObject,
    default = "contentData",
    enums = popup_enums
)]
pub struct Popup {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify, reset)]
    pub z: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(implicit_size))]
    pub width: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(implicit_size))]
    pub height: f64,
    #[qml(notify)]
    pub implicit_width: f64,
    #[qml(notify)]
    pub implicit_height: f64,
    #[qml(notify)]
    pub content_width: f64,
    #[qml(notify)]
    pub content_height: f64,
    #[qml(notify, readonly)]
    pub available_width: f64,
    #[qml(notify, readonly)]
    pub available_height: f64,
    /// The least distance to the edges of the window; negative for none.
    #[qml(notify, reset)]
    pub margins: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub top_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub left_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub right_margin: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_margins))]
    pub bottom_margin: f64,
    #[qml(notify, reset)]
    pub padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub top_padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub left_padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub right_padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub bottom_padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub horizontal_padding: f64,
    #[qml(notify, reset = WIDER_PADDING)]
    pub vertical_padding: f64,
    #[qml(notify, reset)]
    pub top_inset: f64,
    #[qml(notify, reset)]
    pub left_inset: f64,
    #[qml(notify, reset)]
    pub right_inset: f64,
    #[qml(notify, reset)]
    pub bottom_inset: f64,
    #[qml(notify, reset)]
    pub spacing: f64,
    #[qml(notify, readonly)]
    pub implicit_background_width: f64,
    #[qml(notify, readonly)]
    pub implicit_background_height: f64,
    #[qml(notify, readonly)]
    pub implicit_content_width: f64,
    #[qml(notify, readonly)]
    pub implicit_content_height: f64,
    #[qml(notify, holds = "Item")]
    pub background: Option<ObjectId>,
    #[qml(notify, holds = "Item")]
    pub content_item: Option<ObjectId>,
    #[qml(notify, list, append = popup_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
    /// The item the popup is placed from: the one that declares it until
    /// another is given.
    #[qml(notify, reset = ljos_engine::Reset::FollowFromCreation(natural_parent))]
    pub parent: Option<ObjectId>,
    #[qml(notify)]
    pub visible: bool,
    #[qml(notify, readonly)]
    pub opened: bool,
    #[qml(notify)]
    pub modal: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(natural_dim))]
    pub dim: bool,
    /// Whether the popup takes the keyboard when it opens.
    #[qml(notify)]
    pub focus: bool,
    #[qml(notify, readonly)]
    pub active_focus: bool,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub clip: bool,
    #[qml(notify)]
    pub opacity: f64,
    #[qml(notify)]
    pub scale: f64,
    #[qml(notify, enumeration = "TransformOrigin")]
    pub transform_origin: i32,
    #[qml(notify, flags = "ClosePolicy", reset)]
    pub close_policy: i32,
    /// Stored: a popup is always an item of its window here.
    #[qml(notify, enumeration = "PopupType")]
    pub popup_type: i32,
    /// The `Transition` that brings the popup in.
    #[qml(notify)]
    pub enter: Option<ObjectId>,
    /// The `Transition` that takes the popup away.
    #[qml(notify)]
    pub exit: Option<ObjectId>,
    #[qml(notify, reset = item::FONT_RESET)]
    pub font: Font,
    #[qml(group)]
    pub palette: Palette,
    /// Its item's, which has the window's until it is given one.
    #[qml(notify, reset = item::LOCALE_RESET)]
    pub locale: Value,
    #[qml(notify, readonly)]
    pub mirrored: bool,
    #[qml(group)]
    pub anchors: PopupAnchors,
    #[qml(skip)]
    pub(crate) state: Rc<PopupState>,
}

impl Default for Popup {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            width: 0.0,
            height: 0.0,
            implicit_width: 0.0,
            implicit_height: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            available_width: 0.0,
            available_height: 0.0,
            margins: -1.0,
            top_margin: -1.0,
            left_margin: -1.0,
            right_margin: -1.0,
            bottom_margin: -1.0,
            padding: 0.0,
            top_padding: 0.0,
            left_padding: 0.0,
            right_padding: 0.0,
            bottom_padding: 0.0,
            horizontal_padding: 0.0,
            vertical_padding: 0.0,
            top_inset: 0.0,
            left_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
            spacing: 0.0,
            implicit_background_width: 0.0,
            implicit_background_height: 0.0,
            implicit_content_width: 0.0,
            implicit_content_height: 0.0,
            background: None,
            content_item: None,
            content_data: QmlList::default(),
            content_children: QmlList::default(),
            parent: None,
            visible: false,
            opened: false,
            modal: false,
            dim: false,
            focus: false,
            active_focus: false,
            enabled: true,
            clip: false,
            opacity: 1.0,
            scale: 1.0,
            transform_origin: 4,
            close_policy: close_policy::ON_ESCAPE | close_policy::ON_PRESS_OUTSIDE,
            popup_type: 0,
            enter: None,
            exit: None,
            font: Font::default(),
            palette: Palette::default(),
            locale: Value::Undefined,
            mirrored: false,
            anchors: PopupAnchors::default(),
            state: Rc::default(),
        }
    }
}

pub(crate) const POPUP: Host = Host {
    content_item: Popup::CONTENT_ITEM.index,
    content_data: Popup::CONTENT_DATA.index,
    content_children: Popup::CONTENT_CHILDREN.index,
};

fn popup_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    if is_a::<crate::menu::Menu>(cx.engine, cx.this) {
        // What is declared in a menu becomes one of its items.
        return crate::menu::append(cx, child);
    }
    host_append(cx, child, POPUP)
}

#[qml_impl]
impl Popup {
    /// The popup is about to be shown.
    #[qml(signal, name = "aboutToShow")]
    fn about_to_show();
    #[qml(signal, name = "aboutToHide")]
    fn about_to_hide();
    #[qml(signal, name = "opened")]
    fn opened_signal();
    #[qml(signal)]
    fn closed();

    #[qml(invokable)]
    fn open(cx: &Cx) -> Result<(), EngineError> {
        open(cx.engine, cx.this)
    }

    #[qml(invokable)]
    fn close(cx: &Cx) -> Result<(), EngineError> {
        close(cx.engine, cx.this)
    }

    /// Gives the popup the keyboard, as opening it with `focus` does.
    #[qml(invokable, name = "forceActiveFocus")]
    fn force_active_focus(cx: &Cx, reason: Value) -> Result<(), EngineError> {
        let reason = match reason {
            Value::Undefined | Value::Null => FocusReason::Other,
            v => FocusReason::from_qt(v.to_number() as i32),
        };
        if let Some(item) = item_of(cx.engine, cx.this) {
            force_active_focus(cx.engine, item, reason);
        }
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_popup(cx.engine, cx.this)
    }
}

pub(crate) fn state_of(engine: &Engine, popup: ObjectId) -> Option<Rc<PopupState>> {
    engine.with::<Popup, _>(popup, |p| p.state.clone()).ok()
}

/// The item that draws `popup`.
pub(crate) fn item_of(engine: &Engine, popup: ObjectId) -> Option<ObjectId> {
    state_of(engine, popup)?
        .item
        .get()
        .filter(|i| engine.is_alive(*i))
}

/// The popup `item` draws, if it is a popup's item.
pub(crate) fn popup_of(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    if !is_a::<PopupItem>(engine, item) {
        return None;
    }
    engine.owner(item).filter(|p| is_a::<Popup>(engine, *p))
}

/// Whether `popup` is open and on the screen.
pub(crate) fn is_shown(engine: &Engine, popup: ObjectId) -> bool {
    state_of(engine, popup).is_some_and(|s| s.shown.get())
}

fn emit(engine: &Engine, popup: ObjectId, signal: &str) {
    if engine.is_alive(popup) {
        check(engine, signal, engine.emit_by_name(popup, signal, &[]));
    }
}

/// Writes a value as the type's own code does: a binding on the property
/// stays, as `open()` leaves a `visible:` binding alone in Qt.
pub(crate) fn put(
    engine: &Engine,
    obj: ObjectId,
    prop: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    if engine.untrack(|| engine.read(obj, prop)).change_equals(&v) {
        return Ok(());
    }
    engine.write_derived(obj, prop, v)
}

/// Asks for `popup` to be at `x`, `y` from its parent, as a write of both
/// at once: an open popup moves once, and a closed one opens there.
pub(crate) fn move_to(engine: &Engine, popup: ObjectId, x: f64, y: f64) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    // The watches of `x` and `y` do not see a value that is the one
    // read back from the last placing, so what is asked for is kept here.
    state.requested.set((x, y));
    state.placing.set(true);
    let wrote = [(Popup::X.index, x), (Popup::Y.index, y)]
        .into_iter()
        .try_for_each(|(prop, v)| {
            if engine.untrack(|| engine.read(popup, prop)).to_number() == v {
                return Ok(());
            }
            engine.write(popup, prop, Value::Real(v), WriteKind::Imperative)
        });
    state.placing.set(false);
    wrote?;
    engine.untrack(|| position::place(engine, popup))
}

/// What a popup declared in `owner` is placed from: the item it is
/// declared in, the content item of a popup or of a window.
pub(super) fn default_parent(engine: &Engine, popup: ObjectId) -> Option<ObjectId> {
    let mut cur = engine.owner(popup);
    for _ in 0..64 {
        let o = cur?;
        if is_a::<Popup>(engine, o) {
            return object(engine, o, Popup::CONTENT_ITEM.index);
        }
        if is_a::<Window>(engine, o) {
            return object(engine, o, Window::CONTENT_ITEM.index).or(Some(o));
        }
        if is_item(engine, o) {
            return Some(o);
        }
        cur = engine.owner(o);
    }
    None
}

/// `leftPadding = undefined`: as a control's, a side follows its axis and
/// an axis `padding`.
const WIDER_PADDING: ljos_engine::Reset = ljos_engine::Reset::Follow(wider_padding);

fn wider_padding(e: &Engine, popup: ObjectId, p: ljos_engine::PropIndex) -> Value {
    let sides = |a: Prop<f64>, b: Prop<f64>| p == a.index || p == b.index;
    let from = if sides(Popup::LEFT_PADDING, Popup::RIGHT_PADDING) {
        Popup::HORIZONTAL_PADDING
    } else if sides(Popup::TOP_PADDING, Popup::BOTTOM_PADDING) {
        Popup::VERTICAL_PADDING
    } else {
        Popup::PADDING
    };
    e.read(popup, from.index)
}

/// What a side margin is while it has no value of its own.
fn all_margins(e: &Engine, popup: ObjectId, _side: ljos_engine::PropIndex) -> Value {
    e.read(popup, Popup::MARGINS.index)
}

/// A modal popup dims what is behind it unless it is told not to.
fn natural_dim(e: &Engine, popup: ObjectId, _dim: ljos_engine::PropIndex) -> Value {
    Value::Bool(flag(e, popup, Popup::MODAL.index))
}

fn implicit_size(e: &Engine, popup: ObjectId, size: ljos_engine::PropIndex) -> Value {
    Value::Real(natural_size(e, popup, size == Popup::WIDTH.index))
}

/// The size of a popup that was given none along one axis: its implicit
/// size, made to fit between its margins while it is open.
fn natural_size(e: &Engine, popup: ObjectId, horizontal: bool) -> f64 {
    let implicit = if horizontal {
        Popup::IMPLICIT_WIDTH
    } else {
        Popup::IMPLICIT_HEIGHT
    };
    position::fitted(e, popup, horizontal, num(e, popup, implicit.index))
}

fn natural_parent(e: &Engine, popup: ObjectId, _parent: ljos_engine::PropIndex) -> Value {
    default_parent(e, popup).map_or(Value::Null, Value::Object)
}

fn init_popup(e: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(e, popup) else {
        return Ok(());
    };
    settle_parts(e, popup);

    let follow = |prop: Prop<f64>, from: Prop<f64>| {
        bind_default(e, popup, prop.index, move |e| {
            Value::Real(num(e, popup, from.index))
        })
    };
    // As a control's: a side, then the axis, then `padding`.
    follow(Popup::HORIZONTAL_PADDING, Popup::PADDING)?;
    follow(Popup::VERTICAL_PADDING, Popup::PADDING)?;
    follow(Popup::LEFT_PADDING, Popup::HORIZONTAL_PADDING)?;
    follow(Popup::RIGHT_PADDING, Popup::HORIZONTAL_PADDING)?;
    follow(Popup::TOP_PADDING, Popup::VERTICAL_PADDING)?;
    follow(Popup::BOTTOM_PADDING, Popup::VERTICAL_PADDING)?;
    for side in [
        Popup::TOP_MARGIN,
        Popup::LEFT_MARGIN,
        Popup::RIGHT_MARGIN,
        Popup::BOTTOM_MARGIN,
    ] {
        follow(side, Popup::MARGINS)?;
    }
    // A modal popup dims what is behind it unless it is told not to.
    bind_default(e, popup, Popup::DIM.index, move |e| {
        Value::Bool(flag(e, popup, Popup::MODAL.index))
    })?;

    host_complete(e, popup, POPUP)?;
    bind_default(e, popup, Popup::CONTENT_WIDTH.index, move |e| {
        Value::Real(
            crate::menu::content_extent(e, popup, true)
                .unwrap_or_else(|| content_size(e, popup, POPUP, Item::IMPLICIT_WIDTH.index)),
        )
    })?;
    bind_default(e, popup, Popup::CONTENT_HEIGHT.index, move |e| {
        Value::Real(
            crate::menu::content_extent(e, popup, false)
                .unwrap_or_else(|| content_size(e, popup, POPUP, Item::IMPLICIT_HEIGHT.index)),
        )
    })?;

    let item = item::create(e, popup)?;
    state.item.set(Some(item));

    // What the item works out is what the popup reads.
    for (out, from) in [
        (Popup::AVAILABLE_WIDTH.index, Control::AVAILABLE_WIDTH.index),
        (
            Popup::AVAILABLE_HEIGHT.index,
            Control::AVAILABLE_HEIGHT.index,
        ),
        (
            Popup::IMPLICIT_BACKGROUND_WIDTH.index,
            Control::IMPLICIT_BACKGROUND_WIDTH.index,
        ),
        (
            Popup::IMPLICIT_BACKGROUND_HEIGHT.index,
            Control::IMPLICIT_BACKGROUND_HEIGHT.index,
        ),
        (
            Popup::IMPLICIT_CONTENT_WIDTH.index,
            Control::IMPLICIT_CONTENT_WIDTH.index,
        ),
        (
            Popup::IMPLICIT_CONTENT_HEIGHT.index,
            Control::IMPLICIT_CONTENT_HEIGHT.index,
        ),
        (Popup::ACTIVE_FOCUS.index, Item::ACTIVE_FOCUS.index),
        (Popup::MIRRORED.index, Control::MIRRORED.index),
    ] {
        e.bind_initial(popup, out, move |e| e.read(item, from))?;
    }
    bind_default(e, popup, Popup::WIDTH.index, move |e| {
        Value::Real(natural_size(e, popup, true))
    })?;
    bind_default(e, popup, Popup::HEIGHT.index, move |e| {
        Value::Real(natural_size(e, popup, false))
    })?;
    // A drawer is placed by the edges of its window, so it hangs from the
    // window's overlay as soon as it is in one (as in Qt). Reset still
    // gives where it was declared.
    let drawer = drawer::is_drawer(e, popup);
    bind_default(e, popup, Popup::PARENT.index, move |e| {
        let natural = default_parent(e, popup);
        let parent = if drawer {
            natural
                .and_then(|p| ljos_quick::window_of(e, p))
                .and_then(|top| overlay::ensure(e, top).ok())
                .or(natural)
        } else {
            natural
        };
        parent.map_or(Value::Null, Value::Object)
    })?;

    state
        .requested
        .set(e.untrack(|| (num(e, popup, Popup::X.index), num(e, popup, Popup::Y.index))));
    for (prop, is_x) in [(Popup::X.index, true), (Popup::Y.index, false)] {
        let st = state.clone();
        e.watch(popup, prop, move |e, v| {
            if st.placing.get() {
                return;
            }
            let (x, y) = st.requested.get();
            let n = v.to_number();
            st.requested.set(if is_x { (n, y) } else { (x, n) });
            e.untrack(|| check(e, "popup", position::place(e, popup)));
        })?;
    }
    let st = state.clone();
    e.watch(popup, Popup::VISIBLE.index, move |e, v| {
        if !st.switching.get() {
            check(e, "visible", visible_written(e, popup, v.to_boolean()));
        }
    })?;
    guard_visible(e, popup, &state);
    for prop in [Popup::ENTER.index, Popup::EXIT.index] {
        let st = state.clone();
        e.watch(popup, prop, move |e, _| {
            guard_visible(e, popup, &st);
        })?;
    }
    for prop in [Popup::MODAL.index, Popup::DIM.index] {
        e.watch(popup, prop, move |e, _| {
            check(e, "popup", overlay::reshade(e, popup));
        })?;
    }
    item::mirror_look(e, popup, item)?;

    // Where the popup's item lives follows the window its parent is in.
    e.effect_in(popup, move |e| {
        let parent = object(e, popup, Popup::PARENT.index);
        let top = parent.and_then(|p| ljos_quick::window_of(e, p));
        e.untrack(|| check(e, "popup", settle(e, popup, top)));
    })?;
    e.effect_in(popup, move |e| {
        check(e, "popup", position::place(e, popup));
    })?;

    if e.untrack(|| flag(e, popup, Popup::VISIBLE.index)) {
        // Declared open.
        emit(e, popup, "aboutToShow");
        try_present(e, popup)?;
        arrive(e, popup, &state)?;
    }
    Ok(())
}

/// The window the popup's parent is in, if it is in one (untracked).
fn window_top(engine: &Engine, popup: ObjectId) -> Option<ObjectId> {
    engine.untrack(|| {
        let parent = object(engine, popup, Popup::PARENT.index)?;
        ljos_quick::window_of(engine, parent)
    })
}

/// Puts the popup's item into the overlay of `top`, and on the screen if
/// the popup is open.
fn settle(engine: &Engine, popup: ObjectId, top: Option<ObjectId>) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    let home = match top {
        Some(t) => Some(overlay::ensure(engine, t)?),
        None => None,
    };
    if engine.visual_parent(item) != home {
        if state.shown.get() {
            withdraw(engine, popup, &state)?;
        }
        set_parent_item(engine, item, home)?;
    }
    if home.is_some()
        && !state.shown.get()
        && flag(engine, popup, Popup::VISIBLE.index)
        && !crate::tooltip::waiting(engine, popup)
    {
        present(engine, popup, &state)?;
    }
    Ok(())
}

fn try_present(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let top = window_top(engine, popup);
    engine.untrack(|| settle(engine, popup, top))
}

pub(crate) fn open(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    // A popup that is going is still visible, and comes back.
    let going = state.phase.get() == Phase::Going;
    if !going {
        if engine.untrack(|| flag(engine, popup, Popup::VISIBLE.index)) {
            return Ok(());
        }
        if crate::tooltip::defer(engine, popup) {
            // A tool tip that waits for its delay opens when that has
            // passed.
            return Ok(());
        }
    }
    if going {
        // From where it is: what the exit transition did so far stays.
        stop(engine, popup, &state, Popup::EXIT.index);
        state.before_exit.set(None);
    }
    emit(engine, popup, "aboutToShow");
    if !engine.is_alive(popup) {
        return Ok(());
    }
    if going {
        // Qt says so again, though the popup never stopped being visible.
        state.switching.set(true);
        emit(engine, popup, "visibleChanged");
        state.switching.set(false);
        if let Some(item) = item_of(engine, popup) {
            engine.untrack(|| take_keyboard(engine, popup, &state, item));
        }
    } else {
        state.switching.set(true);
        let wrote = put(engine, popup, Popup::VISIBLE.index, Value::Bool(true));
        state.switching.set(false);
        wrote?;
        try_present(engine, popup)?;
    }
    arrive(engine, popup, &state)
}

/// Whether `popup` is on its way out: its exit transition runs.
pub(super) fn is_going(engine: &Engine, popup: ObjectId) -> bool {
    state_of(engine, popup).is_some_and(|s| s.phase.get() == Phase::Going)
}

/// Shows a closed popup where it is, without its enter transition: a
/// drawer that the pointer brings in.
pub(super) fn show_still(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    if engine.untrack(|| flag(engine, popup, Popup::VISIBLE.index)) {
        return Ok(());
    }
    emit(engine, popup, "aboutToShow");
    if !engine.is_alive(popup) {
        return Ok(());
    }
    state.switching.set(true);
    let wrote = put(engine, popup, Popup::VISIBLE.index, Value::Bool(true));
    state.switching.set(false);
    wrote?;
    try_present(engine, popup)
}

/// Holds a popup that is coming where it is: the pointer has it now.
pub(super) fn hold(engine: &Engine, popup: ObjectId) {
    if let Some(state) = state_of(engine, popup)
        && state.phase.get() == Phase::Coming
    {
        stop(engine, popup, &state, Popup::ENTER.index);
    }
}

/// Brings a popup the pointer let go of the rest of the way in with its
/// enter transition. One that was open (`again`) says once more that it
/// is about to show, as Qt does.
pub(super) fn settle_open(
    engine: &Engine,
    popup: ObjectId,
    again: bool,
) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    match state.phase.get() {
        Phase::Coming => stop(engine, popup, &state, Popup::ENTER.index),
        Phase::Going => {
            stop(engine, popup, &state, Popup::EXIT.index);
            state.before_exit.set(None);
        }
        Phase::Rest => {}
    }
    if again {
        emit(engine, popup, "aboutToShow");
        if !engine.is_alive(popup) {
            return Ok(());
        }
    }
    arrive(engine, popup, &state)
}

/// Closes a popup without its exit transition: a drawer let go where
/// nothing of it is in.
pub(super) fn close_at_once(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    if !engine.untrack(|| flag(engine, popup, Popup::VISIBLE.index)) {
        return Ok(());
    }
    for prop in [Popup::ENTER.index, Popup::EXIT.index] {
        stop(engine, popup, &state, prop);
    }
    state.before_exit.set(None);
    emit(engine, popup, "aboutToHide");
    if !engine.is_alive(popup) {
        return Ok(());
    }
    set_computed(engine, popup, Popup::OPENED.index, Value::Bool(false))?;
    gone(engine, popup, &state)
}

/// The `Transition` in `prop` of `popup`.
fn transition(engine: &Engine, popup: ObjectId, prop: PropIndex) -> Option<ObjectId> {
    engine
        .untrack(|| object(engine, popup, prop))
        .filter(|t| is_a::<ljos_quick::Transition>(engine, *t))
}

/// Ends the transition in `prop` where it is.
fn stop(engine: &Engine, popup: ObjectId, state: &PopupState, prop: PropIndex) {
    state.phase.set(Phase::Rest);
    if let Some(t) = transition(engine, popup, prop) {
        ljos_quick::stop_transition(engine, t);
    }
}

/// The popup was shown: it is opened when its enter transition has run,
/// which is at once if it has none or is on no screen.
fn arrive(engine: &Engine, popup: ObjectId, state: &Rc<PopupState>) -> Result<(), EngineError> {
    let enter = transition(engine, popup, Popup::ENTER.index).filter(|_| state.shown.get());
    let Some(enter) = enter else {
        return arrived(engine, popup);
    };
    state.phase.set(Phase::Coming);
    let st = state.clone();
    ljos_quick::run_transition(engine, enter, popup, move |e| {
        if st.phase.get() == Phase::Coming {
            st.phase.set(Phase::Rest);
            check(e, "opened", arrived(e, popup));
        }
    });
    Ok(())
}

fn arrived(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    if !engine.is_alive(popup) {
        return Ok(());
    }
    set_computed(engine, popup, Popup::OPENED.index, Value::Bool(true))?;
    emit(engine, popup, "opened");
    Ok(())
}

/// The popup has gone: it is hidden and says that it is closed.
fn gone(engine: &Engine, popup: ObjectId, state: &PopupState) -> Result<(), EngineError> {
    state.phase.set(Phase::Rest);
    if !engine.is_alive(popup) {
        return Ok(());
    }
    if state.shown.get() {
        engine.untrack(|| withdraw(engine, popup, state))?;
    }
    if let Some((opacity, scale)) = state.before_exit.take() {
        put(engine, popup, Popup::OPACITY.index, Value::Real(opacity))?;
        put(engine, popup, Popup::SCALE.index, Value::Real(scale))?;
    }
    state.switching.set(true);
    let wrote = put(engine, popup, Popup::VISIBLE.index, Value::Bool(false));
    state.switching.set(false);
    wrote?;
    emit(engine, popup, "closed");
    Ok(())
}

/// Stands in front of `visible` while the popup has a transition: a
/// popup that is told to be invisible stays visible until it has gone,
/// one that is told to be visible while it goes comes back, and one that
/// is told to be visible says that it is about to show before it is.
fn guard_visible(engine: &Engine, popup: ObjectId, state: &Rc<PopupState>) {
    let prop = Popup::VISIBLE.index;
    let has = |p| transition(engine, popup, p).is_some();
    if !has(Popup::ENTER.index) && !has(Popup::EXIT.index) {
        engine.set_interceptor(popup, prop, None);
        return;
    }
    // A tool tip that is made visible waits for its delay first, which
    // is its own to see to.
    let waits = is_a::<crate::tooltip::ToolTip>(engine, popup);
    let st = state.clone();
    let guard: Interceptor = Rc::new(move |e, v, how| {
        let on = v.to_boolean();
        let mine = if on {
            st.phase.get() == Phase::Going || (!waits && !flag(e, popup, Popup::VISIBLE.index))
        } else {
            st.shown.get()
                && transition(e, popup, Popup::EXIT.index).is_some()
                && flag(e, popup, Popup::VISIBLE.index)
        };
        if !mine {
            return false;
        }
        let act = move |e: &Engine| {
            let r = if on { open(e, popup) } else { close(e, popup) };
            check(e, "visible", r);
        };
        match how {
            Interception::Write => act(e),
            // Its binding is being worked out: nothing may be written yet.
            Interception::Binding => e.after_binding(act),
        }
        true
    });
    engine.set_interceptor(popup, prop, Some(guard));
}

pub(crate) fn close(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    if crate::tooltip::cancel(engine, popup) {
        // Still waiting for its delay: it was never shown, so there is
        // nothing to hide and nothing to say.
        state.switching.set(true);
        let wrote = put(engine, popup, Popup::VISIBLE.index, Value::Bool(false));
        state.switching.set(false);
        return wrote;
    }
    if !engine.untrack(|| flag(engine, popup, Popup::VISIBLE.index))
        || state.phase.get() == Phase::Going
    {
        return Ok(());
    }
    if state.phase.get() == Phase::Coming {
        // It goes from where it has come to.
        stop(engine, popup, &state, Popup::ENTER.index);
    }
    emit(engine, popup, "aboutToHide");
    if !engine.is_alive(popup) {
        return Ok(());
    }
    set_computed(engine, popup, Popup::OPENED.index, Value::Bool(false))?;
    let exit = transition(engine, popup, Popup::EXIT.index).filter(|_| state.shown.get());
    let Some(exit) = exit else {
        return gone(engine, popup, &state);
    };
    // The keyboard goes back as the popup starts to go: what is typed
    // meanwhile is not for something on its way out.
    if let Some(item) = item_of(engine, popup) {
        engine.untrack(|| give_back_keyboard(engine, &state, item))?;
    }
    state.phase.set(Phase::Going);
    let (opacity, scale) = engine.untrack(|| {
        (
            num(engine, popup, Popup::OPACITY.index),
            num(engine, popup, Popup::SCALE.index),
        )
    });
    state.before_exit.set(Some((opacity, scale)));
    let st = state.clone();
    ljos_quick::run_transition(engine, exit, popup, move |e| {
        if st.phase.get() == Phase::Going {
            check(e, "closed", gone(e, popup, &st));
        }
    });
    Ok(())
}

/// Closes a popup because the user turned away from it: Escape, or a
/// press beside it. A dialog is rejected by that.
pub(crate) fn dismiss(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    if state_of(engine, popup).is_some_and(|s| s.phase.get() == Phase::Going) {
        // Turned away from already.
        return Ok(());
    }
    if is_a::<crate::dialog::Dialog>(engine, popup) {
        crate::dialog::reject(engine, popup)
    } else {
        close(engine, popup)
    }
}

/// `visible` was assigned, or its binding gave a new value: the same as
/// `open()` and `close()`, with the property already written.
pub(crate) fn visible_written(
    engine: &Engine,
    popup: ObjectId,
    on: bool,
) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, popup) else {
        return Ok(());
    };
    // A tool tip waits for its delay, and one that is still waiting has
    // nothing to hide.
    if (on && crate::tooltip::defer(engine, popup))
        || (!on && crate::tooltip::cancel(engine, popup))
    {
        return Ok(());
    }
    if on {
        emit(engine, popup, "aboutToShow");
        try_present(engine, popup)?;
        arrive(engine, popup, &state)?;
    } else {
        // Reached only by a write that `guard_visible` did not see: the
        // popup is invisible already, so there is nothing left to go.
        for prop in [Popup::ENTER.index, Popup::EXIT.index] {
            stop(engine, popup, &state, prop);
        }
        state.before_exit.set(None);
        emit(engine, popup, "aboutToHide");
        set_computed(engine, popup, Popup::OPENED.index, Value::Bool(false))?;
        if state.shown.get() {
            engine.untrack(|| withdraw(engine, popup, &state))?;
        }
        emit(engine, popup, "closed");
    }
    Ok(())
}

/// Shows an open popup whose item is in an overlay.
fn present(engine: &Engine, popup: ObjectId, state: &PopupState) -> Result<(), EngineError> {
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    let Some(home) = engine.visual_parent(item) else {
        return Ok(());
    };
    state.shown.set(true);
    state.pressed_outside.set(false);
    overlay::raise(engine, popup, state, item, home)?;
    position::place(engine, popup)?;
    engine.write(
        item,
        Item::VISIBLE.index,
        Value::Bool(true),
        WriteKind::Imperative,
    )?;
    overlay::changed(engine, home);
    take_keyboard(engine, popup, state, item);
    item::set_modal(engine, popup, item)?;
    let then = state.when_shown.0.borrow().clone();
    if let Some(then) = then {
        then(engine);
    }
    Ok(())
}

/// Gives a shown popup the keyboard, if it wants it, and remembers who
/// had it.
fn take_keyboard(engine: &Engine, popup: ObjectId, state: &PopupState, item: ObjectId) {
    if state.took_focus.get() || !flag(engine, popup, Popup::FOCUS.index) {
        return;
    }
    let Some(home) = engine.visual_parent(item) else {
        return;
    };
    let top = ljos_quick::window_item(engine, home);
    state.saved_focus.set(active_focus_item(engine, top));
    state.took_focus.set(true);
    force_active_focus(engine, item, FocusReason::Popup);
}

/// Gives the keyboard back to who had it before the popup took it.
fn give_back_keyboard(
    engine: &Engine,
    state: &PopupState,
    item: ObjectId,
) -> Result<(), EngineError> {
    if state.took_focus.replace(false) {
        let saved = state.saved_focus.take().filter(|s| engine.is_alive(*s));
        if flag(engine, item, Item::ACTIVE_FOCUS.index) {
            match saved {
                Some(s) => force_active_focus(engine, s, FocusReason::Popup),
                None => engine.write(
                    item,
                    Item::FOCUS.index,
                    Value::Bool(false),
                    WriteKind::Imperative,
                )?,
            }
        }
    }
    Ok(())
}

/// Takes a shown popup off the screen. The keyboard goes back first, so
/// that it never rests on something hidden.
fn withdraw(engine: &Engine, popup: ObjectId, state: &PopupState) -> Result<(), EngineError> {
    state.shown.set(false);
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    give_back_keyboard(engine, state, item)?;
    engine.write(
        item,
        Item::VISIBLE.index,
        Value::Bool(false),
        WriteKind::Imperative,
    )?;
    if let Some(d) = state.dimmer.take() {
        engine.destroy(d);
    }
    if let Some(home) = engine.visual_parent(item) {
        overlay::changed(engine, home);
    }
    item::set_modal(engine, popup, item)
}

pub(crate) fn register(engine: &Engine) {
    item::register(engine);
    overlay::register(engine);
}
