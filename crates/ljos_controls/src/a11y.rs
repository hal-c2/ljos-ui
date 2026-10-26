//! What a control tells an assistive technology when its author said
//! nothing: a role by its type and a name from its text or title
//! (ARCHITECTURE.md §13, "Default roles and names").
//!
//! The roles are the ones the documentation of each type and of
//! `Accessible.role` name; Qt's own cannot be read from QML, where
//! `Accessible.name` of a default reads `""`.
//!
//! How a control is (checked, held down, one of so many rows) goes into
//! its node and not into its `Accessible` properties: Qt 6.11 reads
//! `Accessible.checked` of a checked `CheckBox` as `false`, so QML reads
//! the same here, and whatever the application does write there wins.

use ljos_a11y::accesskit::Toggled;
use ljos_a11y::{NodeData, Role};
use ljos_engine::{Atom, Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::{
    Accessible, AccessibleAction, AccessibleAttached, AccessibleDefaults, AccessibleItem,
    register_accessible_defaults, register_accessible_item,
};

use crate::bars::{BusyIndicator, ProgressBar, ToolSeparator};
use crate::button::{AbstractButton, Button, CheckBox, RadioButton, Switch};
use crate::calendar::{DayOfWeekRow, MonthGrid, WeekNumberColumn};
use crate::combobox::ComboBox;
use crate::containers::{Frame, GroupBox, Page, Pane, ToolBar};
use crate::dial::Dial;
use crate::label::Label;
use crate::menu::MenuSeparator;
use crate::menu_bar::MenuBar;
use crate::page_indicator::PageIndicator;
use crate::scroll_bar::{ScrollBar, ScrollIndicator};
use crate::slider::{RangeSlider, Slider};
use crate::spin_box::{DoubleSpinBox, SpinBox};
use crate::swipe_view::SwipeView;
use crate::tab_bar::{TabBar, TabButton};
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::tumbler::Tumbler;
use crate::util::{bind_default, flag, is_a, is_unset, spoken_without_mnemonic};

/// `Qt.PartiallyChecked`.
const PARTIALLY_CHECKED: i32 = 1;

/// Gives `obj` the role, and the name in `name` (a string property of
/// `obj`), unless its author set them with `Accessible.`.
pub(crate) fn defaults(
    engine: &Engine,
    obj: ObjectId,
    role: Role,
    name: Option<PropIndex>,
) -> Result<(), EngineError> {
    let attacher = MetaRef::Static(Accessible::static_meta());
    let att = engine.attached_object(obj, &attacher)?;
    let role_prop = AccessibleAttached::ROLE.index;
    if is_unset(engine, att, role_prop) {
        // Derived: the author's later `Accessible.role` is still the
        // first assignment.
        engine.write_derived(att, role_prop, Value::Int(role.qt_value()))?;
    }
    if let Some(name) = name {
        bind_default(engine, att, AccessibleAttached::NAME.index, move |e| {
            e.read(obj, name)
        })?;
    }
    Ok(())
}

/// A button is named by its text without the marker of its mnemonic.
/// One that has no text has no name, whatever it shows: Qt names none
/// by its icon or by the text of what is inside it.
pub(crate) fn button_defaults(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let role = if is_a::<crate::menu::MenuItem>(engine, obj)
        || is_a::<crate::menu_bar::MenuBarItem>(engine, obj)
    {
        Role::MenuItem
    } else if is_a::<crate::combobox::ItemDelegate>(engine, obj) {
        Role::ListItem
    } else if is_a::<CheckBox>(engine, obj) {
        Role::CheckBox
    } else if is_a::<RadioButton>(engine, obj) {
        Role::RadioButton
    } else if is_a::<Switch>(engine, obj) {
        Role::Switch
    } else if is_a::<Button>(engine, obj) {
        Role::Button
    } else {
        // A bare `AbstractButton` is whatever its author builds from it;
        // a button is the closest thing to say.
        Role::Button
    };
    defaults(engine, obj, role, None)?;
    let attacher = MetaRef::Static(Accessible::static_meta());
    let att = engine.attached_object(obj, &attacher)?;
    bind_default(engine, att, AccessibleAttached::NAME.index, move |e| {
        let text = e.read(obj, AbstractButton::TEXT.index).to_js_string();
        if text.as_str().contains('&') {
            Value::String(Atom::intern(&spoken_without_mnemonic(text.as_str())))
        } else {
            Value::String(text)
        }
    })
}

/// A button, or a delegate that is a check box, a radio button or a
/// switch and is named by its text as it is.
fn abstract_button_defaults(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    match crate::value_controls::button_role(engine, obj) {
        Some(role) => defaults(engine, obj, role, Some(AbstractButton::TEXT.index)),
        None => button_defaults(engine, obj),
    }
}

/// A container is named by its title when it has one.
pub(crate) fn pane_defaults(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    if is_a::<GroupBox>(engine, obj) {
        defaults(engine, obj, Role::Grouping, Some(GroupBox::TITLE.index))
    } else if is_a::<Page>(engine, obj) {
        defaults(engine, obj, Role::PageTab, Some(Page::TITLE.index))
    } else if is_a::<ToolBar>(engine, obj) {
        defaults(engine, obj, Role::ToolBar, None)
    } else if is_a::<Frame>(engine, obj) {
        defaults(engine, obj, Role::Border, None)
    } else if is_a::<Pane>(engine, obj) {
        defaults(engine, obj, Role::Pane, None)
    } else {
        Ok(())
    }
}

/// A label is static text; the scene names a text item by its text.
pub(crate) fn label_defaults(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    defaults(engine, obj, Role::StaticText, None)
}

/// What a button says of its state. Runs inside the effect that describes
/// the item, so the node follows `checked`, `down` and the rows around it.
fn describe_button(e: &Engine, obj: ObjectId, data: &mut NodeData) {
    let att = e.existing_attached_object(obj, &MetaRef::Static(Accessible::static_meta()));
    // What the application wrote, if it wrote anything, is what is said.
    let own = |p: PropIndex, of_control: bool| match att {
        Some(att) if !is_unset(e, att, p) => flag(e, att, p),
        _ => of_control,
    };
    let checkable = own(
        AccessibleAttached::CHECKABLE.index,
        flag(e, obj, AbstractButton::CHECKABLE.index),
    );
    let checked = own(
        AccessibleAttached::CHECKED.index,
        flag(e, obj, AbstractButton::CHECKED.index),
    );
    let state = if is_a::<CheckBox>(e, obj) {
        Some(CheckBox::CHECK_STATE.index)
    } else if is_a::<crate::delegates::CheckDelegate>(e, obj) {
        Some(crate::delegates::CheckDelegate::CHECK_STATE.index)
    } else {
        None
    };
    let mixed = own(
        AccessibleAttached::CHECK_STATE_MIXED.index,
        state.is_some_and(|p| e.read(obj, p).to_number() as i32 == PARTIALLY_CHECKED),
    );
    data.toggled = (checkable || checked || mixed).then_some(if mixed {
        Toggled::Mixed
    } else if checked {
        Toggled::True
    } else {
        Toggled::False
    });
    data.pressed = own(
        AccessibleAttached::PRESSED.index,
        flag(e, obj, AbstractButton::DOWN.index),
    );
    let said = |p: PropIndex| att.is_some_and(|att| !is_unset(e, att, p));
    if let Some((place, of)) = crate::menu::place_of_item(e, obj) {
        data.position_in_set = Some(place);
        data.size_of_set = Some(of);
    }
    if let Some(row) = crate::combobox::place_of_row(e, obj) {
        data.position_in_set = Some(row.place);
        data.size_of_set = Some(row.of);
        if !said(AccessibleAttached::SELECTED.index) && !said(AccessibleAttached::SELECTABLE.index)
        {
            data.selected = Some(row.picked);
        }
    }
}

/// A press is the scene's to deliver; a button takes nothing else.
fn no_action(_: &Engine, _: ObjectId, _: AccessibleAction) -> bool {
    false
}

/// A type, by its meta-object.
type Meta = fn() -> &'static ljos_engine::MetaObject;

/// The defaults of each type, a base type's before a derived one's, which
/// writes over them. They are given when the `Accessible` object is made,
/// which is when something first asks for it.
const DEFAULTS: &[(Meta, AccessibleDefaults)] = &[
    (AbstractButton::static_meta, abstract_button_defaults),
    (TabButton::static_meta, |e, o| {
        defaults(e, o, Role::PageTab, None)
    }),
    (TabBar::static_meta, |e, o| {
        defaults(e, o, Role::PageTabList, None)
    }),
    (SwipeView::static_meta, |e, o| {
        defaults(e, o, Role::PageTabList, None)
    }),
    (ScrollBar::static_meta, |e, o| {
        defaults(e, o, Role::ScrollBar, None)
    }),
    (ScrollIndicator::static_meta, |e, o| {
        defaults(e, o, Role::Indicator, None)
    }),
    (PageIndicator::static_meta, |e, o| {
        defaults(e, o, Role::Indicator, None)
    }),
    (BusyIndicator::static_meta, |e, o| {
        defaults(e, o, Role::Indicator, None)
    }),
    (ProgressBar::static_meta, |e, o| {
        defaults(e, o, Role::ProgressBar, None)
    }),
    (ToolSeparator::static_meta, |e, o| {
        defaults(e, o, Role::Separator, None)
    }),
    (MenuSeparator::static_meta, |e, o| {
        defaults(e, o, Role::Separator, None)
    }),
    (MenuBar::static_meta, |e, o| {
        defaults(e, o, Role::MenuBar, None)
    }),
    (Dial::static_meta, |e, o| defaults(e, o, Role::Dial, None)),
    (Slider::static_meta, |e, o| {
        defaults(e, o, Role::Slider, None)
    }),
    (RangeSlider::static_meta, |e, o| {
        defaults(e, o, Role::Slider, None)
    }),
    (SpinBox::static_meta, |e, o| {
        defaults(e, o, Role::SpinBox, None)
    }),
    (DoubleSpinBox::static_meta, |e, o| {
        defaults(e, o, Role::SpinBox, None)
    }),
    (Tumbler::static_meta, |e, o| {
        defaults(e, o, Role::SpinBox, None)
    }),
    (ComboBox::static_meta, |e, o| {
        defaults(e, o, Role::ComboBox, Some(ComboBox::DISPLAY_TEXT.index))
    }),
    (MonthGrid::static_meta, |e, o| {
        defaults(e, o, Role::Table, None)
    }),
    (DayOfWeekRow::static_meta, |e, o| {
        defaults(e, o, Role::Row, None)
    }),
    (WeekNumberColumn::static_meta, |e, o| {
        defaults(e, o, Role::Column, None)
    }),
    (Label::static_meta, label_defaults),
    (Pane::static_meta, pane_defaults),
    (
        TextField::static_meta,
        crate::text_control::placeholder_description,
    ),
    (
        TextArea::static_meta,
        crate::text_control::placeholder_description,
    ),
];

pub(crate) fn register(engine: &Engine) {
    for (meta, f) in DEFAULTS {
        register_accessible_defaults(engine, MetaRef::Static(meta()), *f);
    }
    register_accessible_item(
        engine,
        MetaRef::Static(AbstractButton::static_meta()),
        AccessibleItem {
            describe: describe_button,
            act: no_action,
        },
    );
}
