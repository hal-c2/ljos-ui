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
//! `opacity` and `scale` are its item's. With reduced motion what they
//! would grow, shrink or slide is at its end at once, and a popup whose
//! transition does not fade it fades in or out instead
//! ([`ljos_quick::run_passage`]), so `opened` and `closed` come when the
//! fade is over (`popup-transitions-reduced-motion.feature`).

mod drawer;
mod drawer_drag;
mod item;
mod keyboard;
mod kind;
mod lifecycle;
mod overlay;
mod placement;
mod position;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    ConnectionId, Cx, Engine, EngineError, Font, ObjectId, Prop, QmlList, QmlObject, QtObject,
    SignalIndex, Value, WriteKind, qml_impl,
};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::{Item, is_item, set_parent_item};

use crate::containers::{Host, content_size, host_complete};
use crate::control::Control;
use crate::parts::settle_parts;
use crate::util::{bind_default, check, flag, is_a, num, object, set_derived};
use crate::window::Window;
use ljos_quick::Palette;

pub use drawer::Drawer;
pub(crate) use item::{PopupItem, follow, set_role};
pub(crate) use kind::{PopupKind, kind_of};
use lifecycle::{Event, Phase};
pub(crate) use lifecycle::{close, dismiss, is_going, open, visible_written};
use lifecycle::{close_at_once, hold, settle_open, show_still};
pub(crate) use overlay::last_pointer;
pub use overlay::{Overlay, OverlayAttached};
pub(crate) use placement::Flips;

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
    /// The parent whose destruction clears the popup's logical parent.
    parent_destroyed: Cell<Option<(ObjectId, ConnectionId)>>,
    /// `open()` and `close()` write `visible` themselves.
    switching: Cell<bool>,
    /// Which of the popup's transitions is running.
    phase: Cell<Phase>,
    /// What kind of popup it is, once that was asked.
    kind: Cell<Option<PopupKind>>,
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
    /// The axes along which it opens on the other side of its parent when
    /// that has more room, as a combo box's popup and a sub-menu do.
    pub flips: Cell<Flips>,
    /// What the control the popup belongs to does each time the popup is
    /// shown, once it has its size and its place.
    when_shown: WhenShown,
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

/// Lets `popup` open on the other side of its parent, along `flips`, when
/// it does not fit where it was put (`popup/placement.rs`).
pub(crate) fn set_flips(engine: &Engine, popup: ObjectId, flips: Flips) {
    if let Some(state) = state_of(engine, popup) {
        state.flips.set(flips);
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
    #[qml(notify, list, append = popup_append, insert = popup_insert)]
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
    #[qml(notify, reset = crate::inherit::FONT_RESET)]
    pub font: Font,
    #[qml(group, reset)]
    pub palette: Palette,
    /// The window's until it is given one.
    #[qml(notify, reset = crate::inherit::LOCALE_RESET)]
    pub locale: Value,
    #[qml(notify, readonly)]
    pub mirrored: bool,
    #[qml(group)]
    pub anchors: PopupAnchors,
    #[qml(skip)]
    pub(crate) state: Rc<PopupState>,
    #[qml(skip)]
    pub(crate) inherit: Rc<crate::inherit::InheritState>,
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
            inherit: Rc::default(),
        }
    }
}

pub(crate) const POPUP: Host = Host {
    content_item: Popup::CONTENT_ITEM.index,
    content_data: Popup::CONTENT_DATA.index,
    content_children: Popup::CONTENT_CHILDREN.index,
};

fn popup_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    kind_of(cx.engine, cx.this).append(cx, child)
}

fn popup_insert(cx: &Cx, child: ObjectId, anchor: ObjectId) -> Result<(), EngineError> {
    kind_of(cx.engine, cx.this).insert(cx, child, anchor)
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

/// The item that draws `popup`, for a tool that reads the scene (the
/// design audit) and meets popups as items.
pub fn popup_item(engine: &Engine, popup: ObjectId) -> Option<ObjectId> {
    item_of(engine, popup)
}

/// The popup `item` draws, if it is a popup's item.
pub fn item_popup(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    popup_of(engine, item)
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
pub(crate) fn default_parent(engine: &Engine, popup: ObjectId) -> Option<ObjectId> {
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
    // Before the item, which takes its look from the popup.
    crate::inherit::attach(e, popup, crate::inherit::Kind::Popup, true)?;
    let kind = kind::of(e, popup, &state);
    bind_default(e, popup, Popup::CONTENT_WIDTH.index, move |e| {
        Value::Real(
            kind.content_extent(e, popup, true)
                .unwrap_or_else(|| content_size(e, popup, POPUP, Item::IMPLICIT_WIDTH.index)),
        )
    })?;
    bind_default(e, popup, Popup::CONTENT_HEIGHT.index, move |e| {
        Value::Real(
            kind.content_extent(e, popup, false)
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
    let drawer = kind.is_drawer();
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
    lifecycle::guard_visible(e, popup, &state);
    for prop in [Popup::ENTER.index, Popup::EXIT.index] {
        let st = state.clone();
        e.watch(popup, prop, move |e, _| {
            lifecycle::guard_visible(e, popup, &st);
        })?;
    }
    for prop in [Popup::MODAL.index, Popup::DIM.index] {
        e.watch(popup, prop, move |e, _| {
            check(e, "popup", overlay::reshade(e, popup));
        })?;
    }

    // Where the popup's item lives follows the window its parent is in.
    let st = state.clone();
    e.effect_in(popup, move |e| {
        let parent = object(e, popup, Popup::PARENT.index);
        if st.parent_destroyed.get().map(|(p, _)| p) != parent {
            if let Some((_, conn)) = st.parent_destroyed.take() {
                e.disconnect(conn);
            }
            let conn = parent.and_then(|p| {
                e.connect_to(p, SignalIndex::DESTROYED, popup, move |e, _| {
                    check(
                        e,
                        "parent",
                        set_derived(e, popup, Popup::PARENT.index, Value::Null),
                    );
                })
                .map_err(|err| check(e, "parent", Err(err)))
                .ok()
            });
            st.parent_destroyed.set(parent.zip(conn));
        }
        let top = parent.and_then(|p| ljos_quick::window_of(e, p));
        e.untrack(|| check(e, "popup", settle(e, popup, top)));
    })?;
    e.effect_in(popup, move |e| {
        check(e, "popup", position::place(e, popup));
    })?;

    lifecycle::handle(e, popup, Event::Declared)
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
        && !kind::of(engine, popup, &state).is_waiting(engine, popup)
    {
        present(engine, popup, &state)?;
    }
    Ok(())
}

fn try_present(engine: &Engine, popup: ObjectId) -> Result<(), EngineError> {
    let top = window_top(engine, popup);
    engine.untrack(|| settle(engine, popup, top))
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
    take_keyboard(engine, popup, item);
    item::set_modal(engine, popup, item)?;
    let then = state.when_shown.0.borrow().clone();
    if let Some(then) = then {
        then(engine);
    }
    Ok(())
}

/// Gives a shown popup the keyboard, if it wants it and has not taken it
/// yet. Its overlay notes that it took it; what had it is known there.
fn take_keyboard(engine: &Engine, popup: ObjectId, item: ObjectId) {
    if !flag(engine, popup, Popup::FOCUS.index) {
        return;
    }
    let Some(home) = engine.visual_parent(item) else {
        return;
    };
    if overlay::take_keyboard(engine, home, popup) {
        force_active_focus(engine, item, FocusReason::Popup);
    }
}

/// The popup goes: if it has the keyboard, whether it took it or an item
/// in it was given focus, the keyboard goes where its overlay says
/// ([`keyboard`]), and never stays on something on its way out.
fn give_back_keyboard(engine: &Engine, popup: ObjectId, item: ObjectId) -> Result<(), EngineError> {
    let Some(home) = engine.visual_parent(item) else {
        return Ok(());
    };
    let had = flag(engine, item, Item::ACTIVE_FOCUS.index);
    match overlay::give_keyboard(engine, home, popup, had) {
        keyboard::Give::Keep => {}
        keyboard::Give::Popup(to) => {
            if let Some(to) = item_of(engine, to) {
                force_active_focus(engine, to, FocusReason::Popup);
            }
        }
        keyboard::Give::Content(to) => force_active_focus(engine, to, FocusReason::Popup),
        keyboard::Give::Nobody => engine.write(
            item,
            Item::FOCUS.index,
            Value::Bool(false),
            WriteKind::Imperative,
        )?,
    }
    Ok(())
}

/// Escape was pressed for `popup`, by its own item or by the control
/// whose popup it is: it is dismissed if its `closePolicy` says so. False
/// when the key is not its.
pub(crate) fn escape(engine: &Engine, popup: ObjectId) -> bool {
    let policy = engine.untrack(|| engine.read(popup, Popup::CLOSE_POLICY.index));
    if policy.to_number() as i32 & close_policy::ON_ESCAPE == 0 {
        return false;
    }
    check(engine, "close", dismiss(engine, popup));
    true
}

/// Whether `popup` is shown and took the keyboard when it opened, so that
/// what is in it may move the keyboard about: a menu's current item.
pub(crate) fn holds_keyboard(engine: &Engine, popup: ObjectId) -> bool {
    engine.untrack(|| {
        is_shown(engine, popup)
            && item_of(engine, popup)
                .and_then(|i| engine.visual_parent(i))
                .is_some_and(|home| overlay::holds_keyboard(engine, home, popup))
    })
}

/// Takes a shown popup off the screen. The keyboard goes back first, so
/// that it never rests on something hidden.
fn withdraw(engine: &Engine, popup: ObjectId, state: &PopupState) -> Result<(), EngineError> {
    state.shown.set(false);
    let Some(item) = item_of(engine, popup) else {
        return Ok(());
    };
    give_back_keyboard(engine, popup, item)?;
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
        overlay::remove_shown(engine, home, popup);
        overlay::changed(engine, home);
    }
    item::set_modal(engine, popup, item)
}

pub(crate) fn register(engine: &Engine) {
    item::register(engine);
    overlay::register(engine);
}
