//! `Ljos.Controls.Basic`: the controls of Ljós's own API as Qt's Basic
//! style draws them (docs/DIALECT.md §4.16). `Ljos.Controls` is the same
//! controls in Ljós's own style ([`crate::ljos_style`]).
//!
//! The module grows a slice at a time, and a control is in it only once
//! the compat pass can lower a file of Qt's that uses it. Each is the
//! Basic style's control as `QtQuick.Controls` has it, the same file under
//! the same name: a file in Ljós's API and the Qt file it was lowered from
//! make the same objects, drawn the same. `FileDialog` and `FolderDialog`
//! are `QtQuick.Dialogs`' own types under a second name. What the module has decides what
//! the pass lowers (`qml_compat`, through `ljos.txt`), so a control added
//! here is one a file of Qt's may now be lowered with.

use ljos_engine::{Engine, MetaRef, QmlType, Version};
use ljos_platform::{FileDialog, FolderDialog};

use crate::AbstractButton;
use crate::style::BASIC;
use crate::util;

/// The module's name, as a file imports it.
pub const MODULE: &str = "Ljos.Controls.Basic";

/// The controls the module has that the Basic style draws.
pub(crate) const DRAWN: &[&str] = &[
    "ApplicationWindow",
    "Button",
    "CheckBox",
    "ComboBox",
    "Control",
    "DelayButton",
    "Dialog",
    "Frame",
    "GroupBox",
    "ItemDelegate",
    "Label",
    "Menu",
    "MenuItem",
    "MenuSeparator",
    "Page",
    "Pane",
    "Popup",
    "RoundButton",
    "ScrollBar",
    "ScrollView",
    "Slider",
    "SpinBox",
    "Switch",
    "TextArea",
    "TextField",
    "ToolBar",
    "ToolButton",
    "ToolTip",
];

pub(crate) fn register(engine: &Engine, basic: &[String]) {
    let version = Version::new(1, 0);
    // A template draws nothing, so it is the same type in every style.
    engine.register_type(
        MODULE,
        version,
        MetaRef::Static(AbstractButton::static_meta()),
    );
    // QtQuick.Dialogs' own, as they are: the system's dialog when it has
    // one, else the one ljos_platform draws.
    for meta in [FileDialog::static_meta(), FolderDialog::static_meta()] {
        engine.register_type(MODULE, version, MetaRef::Static(meta));
    }
    for name in DRAWN {
        if !basic.iter().any(|n| n == name) {
            // The style's build said why; `register` warns of it.
            continue;
        }
        if let Err(err) = engine.register_qml_type(MODULE, version, &format!("{BASIC}/{name}")) {
            util::warn(engine, format!("{MODULE}.{name}: {err}"));
        }
    }
}
