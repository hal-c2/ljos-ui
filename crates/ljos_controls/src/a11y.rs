//! What a control tells an assistive technology when its author said
//! nothing: a role by its type and a name from its text or title
//! (ARCHITECTURE.md §13, "Default roles and names").
//!
//! The roles are the ones the documentation of each type and of
//! `Accessible.role` name; Qt's own cannot be read from QML, where
//! `Accessible.name` of a default reads `""`.

use ljos_a11y::Role;
use ljos_engine::{Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::{Accessible, AccessibleAttached};

use crate::button::{AbstractButton, Button, CheckBox, RadioButton, Switch};
use crate::containers::{Frame, GroupBox, Page, Pane, ToolBar};
use crate::util::{bind_default, is_a, is_unset};

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
    let role = if is_a::<crate::menu::MenuItem>(engine, obj) {
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
