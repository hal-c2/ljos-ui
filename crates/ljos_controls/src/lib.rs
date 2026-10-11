//! Qt Quick Controls for Ljós.
//!
//! Two layers, as the QML modules have them:
//!
//! - `QtQuick.Templates`: what a control *does*, in Rust. [`Control`]
//!   places a background and a content item and knows how big they want
//!   to be; [`AbstractButton`] is the press, check and click state
//!   machine; [`Pane`] holds other items. A template draws nothing.
//! - `QtQuick.Controls` (and `QtQuick.Controls.Basic`): what a control
//!   *looks like*, in QML. Each type is a file of the Basic style under
//!   `qml/`, written from Qt's documentation and from what running Qt
//!   shows (CLEAN_ROOM.md), compiled to IR when this crate is built and
//!   embedded, so that nothing parses QML at run time.
//!
//! [`register`] adds both to an engine that already has `QtQuick`.
//!
//! Behaviour is specified in `features/controls/`.

mod a11y;
mod action;
mod button;
mod combobox;
mod containers;
mod control;
mod dialog;
mod focus;
mod group;
mod icon_label;
mod inherit;
mod label;
mod menu;
mod menu_bar;
mod palette;
mod parts;
mod popup;
mod rows;
mod scroll_bar;
mod scroll_view;
mod spin_box;
mod style;
mod text_area;
mod text_control;
mod text_field;
mod ticker;
mod tooltip;
mod util;
mod window;

use std::cell::Cell;

use ljos_engine::{Engine, MetaRef, QmlType, Version};

pub use action::Action;
pub use button::{AbstractButton, Button, CheckBox, Icon, RadioButton, Switch, ToolButton};
pub use combobox::{ComboBox, ItemDelegate};
pub use containers::{Frame, GroupBox, Page, Pane, ToolBar};
pub use control::Control;
pub use dialog::{Dialog, DialogButtonBox, DialogButtonBoxAttached};
pub use group::{ButtonGroup, ButtonGroupAttached};
pub use label::Label;
pub use menu::{Menu, MenuItem, MenuSeparator};
pub use menu_bar::{MenuBar, MenuBarItem};
pub use palette::Palette;
pub use popup::{Overlay, OverlayAttached, Popup, PopupAnchors};
pub use scroll_bar::{ScrollBar, ScrollBarAttached, ScrollIndicator, ScrollIndicatorAttached};
pub use scroll_view::ScrollView;
pub use spin_box::{SpinBox, SpinButton};
pub use text_area::TextArea;
pub use text_field::TextField;
pub use tooltip::{ToolTip, ToolTipAttached};
pub use util::CATEGORY;
pub use window::{ApplicationWindow, Window};

use style::{BASIC, StyleLibrary};

/// The modules the style's types are registered in. Qt picks the style
/// behind `QtQuick.Controls` when the application starts; here there is
/// one.
const CONTROLS: [&str; 2] = ["QtQuick.Controls", "QtQuick.Controls.Basic"];

/// Set once the modules are in an engine.
#[derive(Default)]
struct Registered(Cell<bool>);

/// Registers `QtQuick.Templates`, `QtQuick.Controls` and
/// `QtQuick.Controls.Basic`, and `Window` in `QtQuick`. Call it after
/// `ljos_quick::register`; calling it again does nothing.
pub fn register(engine: &Engine) {
    if engine.extension::<Registered>().0.replace(true) {
        return;
    }
    engine.register_module(ljos_engine::module!("QtQuick.Templates" 2.15 => [
        Control, AbstractButton, Button, ToolButton, CheckBox, RadioButton, Switch,
        Pane, Frame, GroupBox, ToolBar, Page, Label,
        Action, ButtonGroup, ApplicationWindow,
        Popup, Overlay, Dialog, DialogButtonBox,
        Menu, MenuItem, MenuSeparator, MenuBar, MenuBarItem,
        ToolTip,
        ComboBox, ItemDelegate,
    ]));
    // Replaces the attached-only `Window` of `ljos_quick` with one that can
    // also be a root; the attached properties are the same type.
    for uri in ["QtQuick", "QtQuick.Window"] {
        engine.register_type(
            uri,
            Version::new(2, 15),
            MetaRef::Static(Window::static_meta()),
        );
    }
    // The button's hooks after the control's: the later registration is
    // the one a button gets.
    control::register(engine);
    button::register(engine);
    label::register(engine);
    icon_label::register(engine);
    a11y::register(engine);
    group::register(engine);
    popup::register(engine);
    tooltip::register(engine);
    combobox::register(engine);
    menu_bar::register(engine);
    for meta in [
        ScrollBar::static_meta(),
        ScrollIndicator::static_meta(),
        ScrollView::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    scroll_bar::register(engine);
    scroll_view::register(engine);
    for meta in [
        TextField::static_meta(),
        TextArea::static_meta(),
        SpinBox::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    text_control::register(engine);
    spin_box::register(engine);

    let (library, undecoded) = StyleLibrary::embedded();
    let names = library.type_names();
    engine.add_component_library(library);
    for bad in undecoded {
        util::warn(engine, format!("the Basic style: {bad}"));
    }
    if names.is_empty() {
        util::warn(
            engine,
            "QtQuick.Controls has no types: the Basic style was not compiled when ljos_controls \
             was built (Qt's type table was missing)",
        );
    }
    for uri in CONTROLS {
        // Types that have no look of their own are the templates.
        for meta in [
            AbstractButton::static_meta(),
            Action::static_meta(),
            ButtonGroup::static_meta(),
            Overlay::static_meta(),
        ] {
            engine.register_type(uri, Version::new(2, 15), MetaRef::Static(meta));
        }
        for name in &names {
            let key = format!("{BASIC}/{name}");
            if let Err(err) = engine.register_qml_type(uri, Version::new(2, 15), &key) {
                util::warn(engine, format!("{uri}.{name}: {err}"));
            }
        }
    }
}
