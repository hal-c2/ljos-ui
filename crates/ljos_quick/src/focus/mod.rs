//! Keyboard focus (ARCHITECTURE §11): the focus tree.
//!
//! Every item is in one *focus scope*: the nearest `FocusScope` above it,
//! or, with none, the tree it is in (for a scene's root, the window). A
//! scope has at most one focus item, the item in it whose `focus` is
//! true. The item with *active* focus is found by starting at the
//! window's focus item and, while that is itself a focus scope, going on
//! to the scope's own focus item. Everything on that path reads
//! `activeFocus: true`; the last one gets the keys.
//!
//! [`tree`] keeps the scopes and the path, [`tab`] the Tab order,
//! [`scope`] has `FocusScope` and [`window`] the attached `Window`.
//!
//! Nothing here knows egui, a scene's render tree or the pointer; the
//! state hangs off the engine, so the module can move to a crate of its
//! own (the architecture's `ljos_input`) as it is.
//!
//! Behaviour follows doc.qt.io/qt-6/qtquick-input-focus.html and is
//! pinned by `features/input/focus*.feature` and `tab-order.feature`.

mod scope;
pub(crate) mod tab;
pub(crate) mod tree;
mod window;

pub use scope::{FocusScope, is_focus_scope, register_focus_scope};
pub use tab::{next_item_in_focus_chain, tab_order_builds};
pub use tree::{
    active_focus_item, focus_reason, force_active_focus, is_window_active, scope_focus_item,
};
pub use window::{Window, WindowAttached};

use ljos_engine::{Engine, EngineError, ObjectId};

/// Why an item got (or lost) active focus: Qt's `Qt.FocusReason`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FocusReason {
    /// A pointer press.
    Mouse = 0,
    /// The Tab key.
    Tab = 1,
    /// Shift+Tab.
    Backtab = 2,
    /// The window became active or inactive.
    ActiveWindow = 3,
    /// A popup took or gave back the keyboard.
    Popup = 4,
    /// A shortcut or a label's mnemonic.
    Shortcut = 5,
    /// The menu bar.
    MenuBar = 6,
    /// Anything else: an assignment to `focus`, `forceActiveFocus()`.
    #[default]
    Other = 7,
}

impl FocusReason {
    /// The reason with Qt's number `n`; anything else is [`Self::Other`].
    pub fn from_qt(n: i32) -> FocusReason {
        match n {
            0 => FocusReason::Mouse,
            1 => FocusReason::Tab,
            2 => FocusReason::Backtab,
            3 => FocusReason::ActiveWindow,
            4 => FocusReason::Popup,
            5 => FocusReason::Shortcut,
            6 => FocusReason::MenuBar,
            _ => FocusReason::Other,
        }
    }
}

/// What an item does for focus when it completes.
pub(crate) fn init(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    tree::init(engine, obj)
}

/// Makes reparented items take their focus along and gives the attached
/// `Window` its properties.
pub(crate) fn register(engine: &Engine) {
    engine.on_visual_parent_changed(tree::reparented);
    window::register(engine);
}
