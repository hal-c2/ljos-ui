//! `Ljos.Controls`: the controls of Ljós's own API (docs/DIALECT.md §4.16).
//!
//! The module grows a slice at a time, and a control is in it only once
//! the compat pass can lower a file of Qt's that uses it. Each is the
//! Basic style's control as `QtQuick.Controls` has it, the same file under
//! the same name: a file in Ljós's API and the Qt file it was lowered from
//! make the same objects, drawn the same. What the module has decides what
//! the pass lowers (`qml_compat`, through `ljos.txt`), so a control added
//! here is one a file of Qt's may now be lowered with.

use ljos_engine::{Engine, MetaRef, QmlType, Version};

use crate::AbstractButton;
use crate::style::BASIC;
use crate::util;

/// The module's name, as a file imports it.
pub const MODULE: &str = "Ljos.Controls";

/// The controls the module has that the Basic style draws.
const DRAWN: &[&str] = &[
    "Button",
    "Label",
    "Popup",
    "ScrollBar",
    "ScrollView",
    "TextArea",
    "TextField",
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
