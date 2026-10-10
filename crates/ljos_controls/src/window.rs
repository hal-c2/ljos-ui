//! `Window` (QtQuick) and `ApplicationWindow` (QtQuick.Templates), as far
//! as hosting a root needs them.
//!
//! Ljós has one surface per view and the host owns it, so a window here
//! is the root item of the scene: it fills the view, paints its `color`
//! and is where the fonts and palettes of the controls inside it start.
//! Its `title` and size are there for the host to read.
//!
//! Differences from Qt, by design of this stage: a window is an item (it
//! has `x`, `anchors` and the rest), and it is visible without being
//! shown. Documentation: doc.qt.io/qt-6/qml-qtquick-window.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-applicationwindow.html; pinned by
//! `features/controls/application-window.feature`.

use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, Font, ObjectId, QmlList, QmlObject, Value, WriteKind, qml_impl,
};
use ljos_quick::focus::WindowAttached;
use ljos_quick::{Item, Rectangle};

use crate::containers::{Host, bar_height, host_append, host_complete};
use crate::inherit::{self, InheritState, Kind};
use crate::palette::Palette;
use crate::parts::{PartKind, destroy_replaced_style_parts, manage_part};
use crate::util::{is_a, num};

/// `Window`.
///
/// It is a `Rectangle` underneath so that its `color` paints.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Window",
    extends = Rectangle,
    attached = WindowAttached
)]
pub struct Window {
    #[qml(base)]
    pub rect: Rectangle,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub minimum_width: i32,
    #[qml(notify)]
    pub minimum_height: i32,
    #[qml(notify)]
    pub maximum_width: i32,
    #[qml(notify)]
    pub maximum_height: i32,
    /// Stored; the host decides what kind of window it is.
    #[qml(notify)]
    pub flags: i32,
    #[qml(notify)]
    pub modality: i32,
    #[qml(notify)]
    pub visibility: i32,
    #[qml(notify, readonly)]
    pub active: bool,
    /// Where the window's children are: the window itself, or for an
    /// `ApplicationWindow` the item between its header and its footer.
    #[qml(notify, readonly)]
    pub content_item: Option<ObjectId>,
    #[qml(group)]
    pub palette: Palette,
    #[qml(skip)]
    pub(crate) inherit: Rc<InheritState>,
}

#[qml_impl]
impl Window {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        if is_a::<ApplicationWindow>(e, obj) {
            // Its own hook does all of this, with its own font.
            return Ok(());
        }
        e.write_internal(
            obj,
            Window::CONTENT_ITEM.index,
            Value::Object(obj),
            WriteKind::Initial,
        )?;
        inherit::attach(e, obj, Kind::Window, true)
    }
}

/// `ApplicationWindow`: a window with a header, a footer and a
/// background, whose children go between the two bars.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ApplicationWindow",
    extends = Window,
    default = "contentData"
)]
pub struct ApplicationWindow {
    #[qml(base)]
    pub window: Window,
    #[qml(notify)]
    pub font: Font,
    /// Accepted and stored.
    #[qml(notify)]
    pub locale: Value,
    #[qml(notify)]
    pub background: Option<ObjectId>,
    #[qml(notify)]
    pub header: Option<ObjectId>,
    #[qml(notify)]
    pub footer: Option<ObjectId>,
    /// Stored: there is no `MenuBar` yet.
    #[qml(notify)]
    pub menu_bar: Option<ObjectId>,
    #[qml(notify, list, append = window_append)]
    pub content_data: QmlList,
    #[qml(notify, list)]
    pub content_children: QmlList,
}

const HOST: Host = Host {
    content_item: Window::CONTENT_ITEM.index,
    content_data: ApplicationWindow::CONTENT_DATA.index,
    content_children: ApplicationWindow::CONTENT_CHILDREN.index,
};

fn window_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    host_append(cx, child, HOST)
}

/// Where the content of an application window is: all of it but the two
/// bars (tracked).
fn content_rect(engine: &Engine, win: ObjectId) -> [f64; 4] {
    let top = bar_height(engine, win, ApplicationWindow::HEADER.index);
    let bottom = bar_height(engine, win, ApplicationWindow::FOOTER.index);
    [
        0.0,
        top,
        num(engine, win, Item::WIDTH.index),
        (num(engine, win, Item::HEIGHT.index) - top - bottom).max(0.0),
    ]
}

#[qml_impl]
impl ApplicationWindow {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        destroy_replaced_style_parts(e, obj);
        host_complete(e, obj, HOST)?;
        manage_part(
            e,
            obj,
            ApplicationWindow::BACKGROUND.index,
            PartKind::Background(None),
        )?;
        manage_part(
            e,
            obj,
            Window::CONTENT_ITEM.index,
            PartKind::Content(content_rect),
        )?;
        manage_part(e, obj, ApplicationWindow::HEADER.index, PartKind::Header)?;
        manage_part(e, obj, ApplicationWindow::FOOTER.index, PartKind::Footer)?;
        inherit::attach(e, obj, Kind::ApplicationWindow, true)
    }
}
