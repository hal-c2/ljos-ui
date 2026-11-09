//! `Ljos.Controls`: the controls of Ljós's API in Ljós's own style
//! (docs/DIALECT.md §4.16).
//!
//! The style is written in QML under `qml/Ljos/Controls`, drawn with the
//! roles of its `Theme` singleton, which derives them as
//! [`ljos_design::roles`] does. Basic's look stays in `Ljos.Controls.Basic`
//! ([`crate::ljos`]), which is what a file lowered from Qt imports.
//! The controls this style has no look of its own for yet are Basic's
//! files under the same names, so a file can move from one module to the
//! other by its import alone.

use ljos_engine::{Engine, MetaRef, QmlType, Version};
use ljos_platform::{FileDialog, FolderDialog};

use crate::AbstractButton;
use crate::ljos::DRAWN;
use crate::style::BASIC;
use crate::util;

/// The module's name, as a file imports it.
pub const MODULE: &str = "Ljos.Controls";

/// The style's directory in the component library.
pub(crate) const DIRECTORY: &str = "Ljos/Controls";

pub(crate) fn register(engine: &Engine, own: &[String], basic: &[String]) {
    let version = Version::new(1, 0);
    // A template draws nothing, so it is the same type in every style.
    engine.register_type(
        MODULE,
        version,
        MetaRef::Static(AbstractButton::static_meta()),
    );
    // The same dialogs as `Ljos.Controls.Basic`: the system draws them.
    for meta in [FileDialog::static_meta(), FolderDialog::static_meta()] {
        engine.register_type(MODULE, version, MetaRef::Static(meta));
    }
    if own.is_empty() {
        util::warn(
            engine,
            format!(
                "{MODULE} has no types: its style was not compiled when ljos_controls was \
                 built (its build script says why)"
            ),
        );
    }
    // What the style has no look of its own for yet is Basic's, so the
    // module has every control `Ljos.Controls.Basic` has.
    let borrowed = DRAWN
        .iter()
        .filter(|n| !own.iter().any(|o| o == *n) && basic.iter().any(|b| b == *n))
        .map(|n| format!("{BASIC}/{n}"));
    for key in own
        .iter()
        .map(|n| format!("{DIRECTORY}/{n}"))
        .chain(borrowed)
    {
        if let Err(err) = engine.register_qml_type(MODULE, version, &key) {
            util::warn(engine, format!("{MODULE}: {err}"));
        }
    }
}
