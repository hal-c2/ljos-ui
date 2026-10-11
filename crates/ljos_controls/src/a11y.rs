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
use ljos_engine::{Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::{
    Accessible, AccessibleAction, AccessibleAttached, AccessibleItem, register_accessible_item,
};

use crate::button::{AbstractButton, Button, CheckBox, RadioButton, Switch};
use crate::containers::{Frame, GroupBox, Page, Pane, ToolBar};
use crate::util::{bind_default, flag, is_a, is_unset};

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

/// A button is named by its text.
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
    defaults(engine, obj, role, Some(AbstractButton::TEXT.index))
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
    let mixed = own(
        AccessibleAttached::CHECK_STATE_MIXED.index,
        is_a::<CheckBox>(e, obj)
            && e.read(obj, CheckBox::CHECK_STATE.index).to_number() as i32 == PARTIALLY_CHECKED,
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

pub(crate) fn register(engine: &Engine) {
    register_accessible_item(
        engine,
        MetaRef::Static(AbstractButton::static_meta()),
        AccessibleItem {
            describe: describe_button,
            act: no_action,
        },
    );
}
