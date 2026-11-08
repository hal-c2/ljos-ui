//! `Qt.labs.platform`'s `MenuBar`, `Menu`, `MenuItem` and `MenuSeparator`:
//! a menu bar of the system's own, for any window (a plain `Window` too).
//!
//! Where the system has a menu bar (macOS), the bar is described to it as
//! a `QtQuick.Controls` `MenuBar` is ([`crate::system_menu`]); elsewhere
//! these types make nothing and say nothing, as Qt's do where the platform
//! has no such bar. Documentation:
//! doc.qt.io/qt-6/qml-qt-labs-platform-menubar.html and its siblings;
//! pinned by `features/controls/platform-menu-bar.feature` and
//! `platform-menu-bar-windows.feature`.

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ObjectId, QmlList, QmlObject, QtObject, Value, qml_impl,
};

/// `MenuBar` (Qt.labs.platform).
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "Qt.labs.platform",
    name = "MenuBar",
    extends = QtObject,
    default = "menus"
)]
pub struct PlatformMenuBar {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify, list)]
    pub menus: QmlList,
    /// The window whose bar this is; unset, the window it is declared in.
    #[qml(notify, object = "Window")]
    pub window: Option<ObjectId>,
}

#[qml_impl]
impl PlatformMenuBar {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        crate::system_menu::adopt_platform(cx.engine, cx.this)
    }
}

/// `Menu` (Qt.labs.platform): in a bar, or in another menu as a sub-menu.
#[derive(Debug, QmlObject)]
#[qml(
    module = "Qt.labs.platform",
    name = "Menu",
    extends = QtObject,
    default = "items"
)]
pub struct PlatformMenu {
    #[qml(base)]
    pub object: QtObject,
    /// `MenuItem`s, `MenuSeparator`s and sub-menus, in order.
    #[qml(notify, list)]
    pub items: QmlList,
    #[qml(notify)]
    pub title: Atom,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub visible: bool,
}

impl Default for PlatformMenu {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            items: QmlList::default(),
            title: Atom::EMPTY,
            enabled: true,
            visible: true,
        }
    }
}

/// `MenuItem.role`: `QAction::MenuRole`'s numbers.
pub(crate) mod role {
    pub const NO_ROLE: i32 = 0;
    pub const TEXT_HEURISTIC_ROLE: i32 = 1;
    pub const APPLICATION_SPECIFIC_ROLE: i32 = 2;
    pub const ABOUT_QT_ROLE: i32 = 3;
    pub const ABOUT_ROLE: i32 = 4;
    pub const PREFERENCES_ROLE: i32 = 5;
    pub const QUIT_ROLE: i32 = 6;
}

fn menu_item_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    use role::*;
    vec![(
        "MenuRole",
        [
            ("NoRole", NO_ROLE),
            ("TextHeuristicRole", TEXT_HEURISTIC_ROLE),
            ("ApplicationSpecificRole", APPLICATION_SPECIFIC_ROLE),
            ("AboutQtRole", ABOUT_QT_ROLE),
            ("AboutRole", ABOUT_ROLE),
            ("PreferencesRole", PREFERENCES_ROLE),
            ("QuitRole", QUIT_ROLE),
        ]
        .into_iter()
        .map(|(k, v)| (k, i64::from(v)))
        .collect(),
    )]
}

/// `MenuItem` (Qt.labs.platform).
#[derive(Debug, QmlObject)]
#[qml(
    module = "Qt.labs.platform",
    name = "MenuItem",
    extends = QtObject,
    enums = menu_item_enums
)]
pub struct PlatformMenuItem {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub visible: bool,
    #[qml(notify)]
    pub checkable: bool,
    #[qml(notify)]
    pub checked: bool,
    /// A `MenuSeparator` is an item that is a separator.
    #[qml(notify)]
    pub separator: bool,
    /// A sequence (`"Ctrl+N"`, `Ctrl` being Command on macOS) or a
    /// `StandardKey`.
    #[qml(notify)]
    pub shortcut: Value,
    /// Where the system puts the item: `MenuItem.TextHeuristicRole` (its
    /// text says), `AboutRole`, `PreferencesRole` and `QuitRole` (the
    /// application menu), or `NoRole` (its own menu).
    #[qml(notify, enumeration = "MenuRole")]
    pub role: i32,
}

impl Default for PlatformMenuItem {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            text: Atom::EMPTY,
            enabled: true,
            visible: true,
            checkable: false,
            checked: false,
            separator: false,
            shortcut: Value::Undefined,
            role: role::TEXT_HEURISTIC_ROLE,
        }
    }
}

#[qml_impl]
impl PlatformMenuItem {
    /// The user chose the item.
    #[qml(signal)]
    fn triggered();
    /// Turns a checkable item's `checked` over.
    #[qml(invokable)]
    fn toggle(cx: &Cx) -> Result<(), EngineError> {
        let e = cx.engine;
        if e.untrack(|| cx.get(PlatformMenuItem::CHECKABLE))? {
            let on = e.untrack(|| cx.get(PlatformMenuItem::CHECKED))?;
            cx.set(PlatformMenuItem::CHECKED, !on)?;
        }
        Ok(())
    }
}

/// `MenuSeparator` (Qt.labs.platform): an item that is a separator.
#[derive(Debug, QmlObject)]
#[qml(
    module = "Qt.labs.platform",
    name = "MenuSeparator",
    extends = PlatformMenuItem
)]
pub struct PlatformMenuSeparator {
    #[qml(base)]
    pub item: PlatformMenuItem,
}

impl Default for PlatformMenuSeparator {
    fn default() -> Self {
        Self {
            item: PlatformMenuItem {
                separator: true,
                ..PlatformMenuItem::default()
            },
        }
    }
}

/// The user chose `item` in the system's menu bar: a checkable one is
/// turned over, then it is triggered. A disabled one is not.
pub(crate) fn trigger(e: &Engine, item: ObjectId) -> Result<(), EngineError> {
    if !crate::util::flag(e, item, PlatformMenuItem::ENABLED.index) {
        return Ok(());
    }
    let _ = e.invoke_by_name(item, "toggle", &[])?;
    if e.is_alive(item) {
        e.emit_by_name(item, "triggered", &[])?;
    }
    Ok(())
}
