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
mod action_group;
mod bars;
mod button;
mod calendar;
mod combobox;
mod container;
mod containers;
mod control;
mod delay_button;
mod delegates;
mod dial;
mod dialog;
mod focus;
mod group;
mod header_view;
mod icon_label;
mod inherit;
mod label;
mod menu;
mod menu_bar;
mod page_indicator;
mod palette;
mod parts;
mod popup;
mod rows;
mod scroll_bar;
mod scroll_view;
mod selection_rectangle;
mod slider;
mod spin_box;
mod split_view;
mod stack_view;
mod style;
mod style_font;
mod swipe_delegate;
mod swipe_view;
mod tab_bar;
mod text_area;
mod text_control;
mod text_field;
mod ticker;
mod tooltip;
mod tumbler;
mod util;
mod value_controls;
mod window;

use std::cell::Cell;

use ljos_engine::{Engine, MetaRef, QmlType, Version};

pub use action::Action;
pub use action_group::{ActionGroup, ActionGroupAttached};
pub use bars::{BusyIndicator, ProgressBar, ToolSeparator};
pub use button::{AbstractButton, Button, CheckBox, Icon, RadioButton, Switch, ToolButton};
pub use calendar::{Calendar, CalendarModel, DayOfWeekRow, MonthGrid, WeekNumberColumn};
pub use combobox::{ComboBox, ItemDelegate};
pub use container::Container;
pub use containers::{Frame, GroupBox, Page, Pane, ToolBar};
pub use control::Control;
pub use delay_button::{DelayButton, RoundButton};
pub use delegates::{CheckDelegate, RadioDelegate, SwitchDelegate, TreeViewDelegate};
pub use dial::Dial;
pub use dialog::{Dialog, DialogButtonBox, DialogButtonBoxAttached};
pub use group::{ButtonGroup, ButtonGroupAttached};
pub use header_view::{
    HeaderViewBase, HeaderViewDelegate, HorizontalHeaderView, TableViewDelegate, VerticalHeaderView,
};
pub use label::Label;
pub use menu::{Menu, MenuItem, MenuSeparator};
pub use menu_bar::{MenuBar, MenuBarItem};
pub use page_indicator::PageIndicator;
pub use palette::Palette;
pub use popup::{Drawer, Overlay, OverlayAttached, Popup, PopupAnchors};
pub use scroll_bar::{ScrollBar, ScrollBarAttached, ScrollIndicator, ScrollIndicatorAttached};
pub use scroll_view::ScrollView;
pub use selection_rectangle::{SelectionRectangle, SelectionRectangleAttached};
pub use slider::{RangeSlider, RangeSliderNode, Slider};
pub use spin_box::{DoubleSpinBox, SpinBox, SpinButton};
pub use split_view::{SplitHandle, SplitHandleAttached, SplitView, SplitViewAttached};
pub use stack_view::{StackView, StackViewAttached};
pub use swipe_delegate::{SwipeDelegate, SwipeDelegateAttached, SwipeDelegateSwipe};
pub use swipe_view::{SwipeView, SwipeViewAttached};
pub use tab_bar::{TabBar, TabBarAttached, TabButton};
pub use text_area::TextArea;
pub use text_field::TextField;
pub use tooltip::{ToolTip, ToolTipAttached};
pub use tumbler::{Tumbler, TumblerAttached};
pub use util::CATEGORY;
pub use window::{
    Application, ApplicationState, ApplicationWindow, CloseEvent, FRAMELESS, KIND_MASK, Modality,
    ResizeEdges, STAYS_ON_BOTTOM, STAYS_ON_TOP, Screen, ScreenAttached, ScreenInfo, ScreenSpec,
    Window, WindowBackdrop, WindowRequest, WindowShow, WindowSpec, WindowSystem,
    application_reopened, device_pixel_ratio, quit_on_last_window_closed,
    set_application_arguments, set_quit_on_last_window_closed,
};

use style::{BASIC, FUSION, MATERIAL, StyleLibrary, UNIVERSAL};

/// The modules the style's types are registered in. Qt picks the style
/// behind `QtQuick.Controls` when the application starts; here that URI
/// uses Basic, and explicit style imports use their own libraries.
const CONTROLS: [&str; 5] = [
    "QtQuick.Controls",
    "QtQuick.Controls.Basic",
    "QtQuick.Controls.Material",
    "QtQuick.Controls.Fusion",
    "QtQuick.Controls.Universal",
];

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
    // A file that imports `QtQuick.Window` has what `QtQuick` has.
    engine.register_import("QtQuick.Window", "QtQuick");
    // The button's hooks after the control's: the later registration is
    // the one a button gets.
    window::register(engine);
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
    // Only asked while a reload is tracked (`ljos run --watch`).
    engine.register_migrate::<StackView>();
    parts::register(
        engine,
        &[
            Control::static_meta(),
            Label::static_meta(),
            Popup::static_meta(),
            ApplicationWindow::static_meta(),
            TextField::static_meta(),
            TextArea::static_meta(),
        ],
    );
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
        DoubleSpinBox::static_meta(),
    ] {
        engine.register_type(
            "QtQuick.Templates",
            Version::new(2, 15),
            MetaRef::Static(meta),
        );
    }
    text_control::register(engine);
    spin_box::register(engine);
    value_controls::register(engine);
    header_view::register(engine);
    selection_rectangle::register(engine);
    container::register(engine);
    calendar::register_all(engine);

    let (libraries, undecoded) = StyleLibrary::embedded();
    let mut styles = std::collections::HashMap::new();
    for (directory, library) in libraries {
        styles.insert(directory, library.type_names());
        engine.add_component_library(library);
    }
    for bad in undecoded {
        util::warn(engine, format!("the built-in styles: {bad}"));
    }
    if styles.get(BASIC).is_none_or(Vec::is_empty) {
        util::warn(
            engine,
            "QtQuick.Controls has no types: the Basic style was not compiled when ljos_controls \
             was built (its build script says why)",
        );
    }
    style::register(engine);
    for uri in CONTROLS {
        let directory = match uri {
            "QtQuick.Controls.Material" => MATERIAL,
            "QtQuick.Controls.Fusion" => FUSION,
            "QtQuick.Controls.Universal" => UNIVERSAL,
            _ => BASIC,
        };
        // Types that have no look of their own are the templates.
        for meta in [
            AbstractButton::static_meta(),
            Action::static_meta(),
            ButtonGroup::static_meta(),
            Overlay::static_meta(),
            Calendar::static_meta(),
            CalendarModel::static_meta(),
        ] {
            engine.register_type(uri, Version::new(2, 15), MetaRef::Static(meta));
        }
        // A style holds only the controls it draws differently; the rest
        // are Basic's.
        let own = styles.get(directory).cloned().unwrap_or_default();
        let basic = styles.get(BASIC).cloned().unwrap_or_default();
        let extra = own.iter().filter(|n| !basic.contains(n));
        for name in basic.iter().chain(extra) {
            let from = if own.contains(name) { directory } else { BASIC };
            let key = format!("{from}/{name}");
            match engine.register_qml_type(uri, Version::new(2, 15), &key) {
                Ok(()) => style_font::remember(engine, uri, name, from),
                Err(err) => util::warn(engine, format!("{uri}.{name}: {err}")),
            }
        }
    }
    // The parts a style draws with, for controls that customise it.
    let parts = format!("{BASIC}/impl");
    for name in styles.get(&parts).cloned().unwrap_or_default() {
        let uri = "QtQuick.Controls.Basic.impl";
        if let Err(err) =
            engine.register_qml_type(uri, Version::new(2, 15), &format!("{parts}/{name}"))
        {
            util::warn(engine, format!("{uri}.{name}: {err}"));
        }
    }
}
