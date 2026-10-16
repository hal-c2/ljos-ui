//! The attached `Window`: what an item can ask about the window it is in.
//!
//! What focus needs, `Window.activeFocusItem` and `Window.active`, is
//! bound here (doc.qt.io/qt-6/qml-qtquick-window.html, attached
//! properties). A scene is the window; the type cannot be created. The
//! rest (`window`, `contentItem`, `width`, `height`, `visibility`) is bound
//! by whoever provides the `Window` type, which knows what a window is.

use ljos_engine::{Engine, LogLevel, ObjectId, QmlObject, Value};

use super::tree;
use crate::item::is_a;

/// `Window` (QtQuick), attached only.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Window",
    attached = WindowAttached,
    uncreatable
)]
pub struct Window {}

/// What `Window.` attaches to an item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "WindowAttached", enums = visibility_enum)]
pub struct WindowAttached {
    /// The item with active focus in the item's window; null when there
    /// is none, the window is inactive or the item is in no window.
    #[qml(notify, readonly)]
    pub active_focus_item: Option<ObjectId>,
    /// Whether the item's window has the keyboard.
    #[qml(notify, readonly)]
    pub active: bool,
    /// The window the item is in; null when it is in none.
    #[qml(notify, readonly)]
    pub window: Option<ObjectId>,
    /// Where the window's items are.
    #[qml(notify, readonly)]
    pub content_item: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub width: i32,
    #[qml(notify, readonly)]
    pub height: i32,
    /// How the window is shown, a `Window.Visibility`.
    #[qml(notify, readonly, enumeration = "Visibility")]
    pub visibility: i32,
}

/// `Window.Visibility`, which `visibility` holds; the `Window` type that
/// is created declares it too.
fn visibility_enum() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Visibility",
        vec![
            ("Hidden", 0),
            ("AutomaticVisibility", 1),
            ("Windowed", 2),
            ("Minimized", 3),
            ("Maximized", 4),
            ("FullScreen", 5),
        ],
    )]
}

/// Binds the attached properties to the window the item is in. The
/// bindings read the item's parents, so they follow it to another window.
pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, obj, att| {
        if !is_a::<WindowAttached>(e, att) {
            return true;
        }
        let focus = e.bind_initial(att, WindowAttached::ACTIVE_FOCUS_ITEM.index, move |e| {
            let root = tree::top_of(e, obj);
            tree::window_nodes(e, root)
                .and_then(|(n, _)| e.graph().try_get(n))
                .filter(|v| v.as_object().is_some_and(|o| e.is_alive(o)))
                .unwrap_or(Value::Null)
        });
        let active = e.bind_initial(att, WindowAttached::ACTIVE.index, move |e| {
            let root = tree::top_of(e, obj);
            tree::window_nodes(e, root)
                .and_then(|(_, n)| e.graph().try_get(n))
                .unwrap_or(Value::Bool(false))
        });
        if let Err(err) = focus.and(active) {
            e.log(
                LogLevel::Warning,
                "qml",
                format!("Window: cannot bind the attached properties: {err}"),
            );
        }
        true
    });
}
