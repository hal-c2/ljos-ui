//! The font a style gives its controls: Material's and Universal's are a
//! little larger than Basic's, and some controls have another size or
//! weight of it. The values are what Qt 6.11 reports for a control nobody
//! gave a font (`features/controls/style-font.feature`).
//!
//! The style of an object is the style whose file its type is, else the
//! style of the control above it: a style holds only the controls it draws
//! differently and has Basic's files for the rest, and those files are the
//! same types in every style. Basic and Fusion have no font of their own.
//!
//! [`crate::inherit`] starts from this font and lays over it only the
//! attributes an author set above, as Qt's controls do.

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_engine::{Engine, Font, MetaRef, ObjectId};

/// A style that has a font of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    Material,
    Universal,
}

/// The types of the built-in style files, by the address of their
/// description: the style each is a file of, `None` for Basic's.
#[derive(Default)]
struct StyleTypes(RefCell<HashMap<usize, Option<Look>>>);

fn address(meta: &MetaRef) -> usize {
    std::ptr::from_ref(&**meta) as usize
}

/// Notes that `name` of `uri` is the file of a built-in style in
/// `directory`.
pub(crate) fn remember(engine: &Engine, uri: &str, name: &str, directory: &str) {
    let Some(meta) = engine.lookup_type(uri, name) else {
        return;
    };
    let look = match directory {
        crate::style::MATERIAL => Some(Look::Material),
        crate::style::UNIVERSAL => Some(Look::Universal),
        _ => None,
    };
    engine
        .extension::<StyleTypes>()
        .0
        .borrow_mut()
        .insert(address(&meta), look);
}

/// The controls whose font differs from the style's own, nearest type
/// first: a `CheckDelegate` is not an `ItemDelegate` here.
const NAMED: &[&str] = &[
    "CheckDelegate",
    "RadioDelegate",
    "SwitchDelegate",
    "SwipeDelegate",
    "ItemDelegate",
    "MenuBarItem",
    "MenuItem",
    "MenuBar",
    "TextField",
    "TextArea",
    "ComboBox",
    "SpinBox",
    "RoundButton",
    "DelayButton",
    "ToolButton",
    "TabButton",
    "Button",
    "TabBar",
    "ToolBar",
    "ToolSeparator",
    "GroupBox",
];

/// What an object's type says of its font: the style whose file it is,
/// if any, and the nearest of [`NAMED`] it is.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Scope {
    pub look: Option<Look>,
    pub name: Option<&'static str>,
}

/// The [`Scope`] of `obj`'s type.
pub(crate) fn scope_of(engine: &Engine, obj: ObjectId) -> Scope {
    let mut scope = Scope::default();
    let types = engine.extension::<StyleTypes>();
    let types = types.0.borrow();
    if types.is_empty() {
        return scope;
    }
    let mut cur = engine.meta_of(obj);
    let mut found = None;
    while let Some(meta) = cur {
        let known = types.get(&address(&meta));
        if scope.look.is_none()
            && let Some(Some(look)) = known
        {
            scope.look = Some(*look);
        }
        // An application's own type of the same name is not the control.
        if found.is_none() && (known.is_some() || meta.native.is_some()) {
            found = NAMED.iter().find(|n| meta.name.as_str() == **n).copied();
        }
        if scope.look.is_some() && found.is_some() {
            break;
        }
        cur = meta.base.clone();
    }
    scope.name = found;
    scope
}

/// The font `look` gives a control that is the type `name`.
pub(crate) fn font(look: Look, name: Option<&str>) -> Font {
    let mut f = Font::default();
    // Point sizes: Qt reports both units, and a size in points follows the
    // display's density as the style's does.
    let (points, weight) = match (look, name.unwrap_or("")) {
        (
            Look::Material,
            "CheckDelegate" | "RadioDelegate" | "SwitchDelegate" | "SwipeDelegate" | "MenuBarItem"
            | "MenuItem" | "MenuBar" | "TextField" | "TextArea" | "ComboBox" | "SpinBox",
        ) => (12.0, 400),
        (
            Look::Material,
            "ItemDelegate" | "RoundButton" | "DelayButton" | "ToolButton" | "TabButton" | "Button"
            | "TabBar" | "ToolBar" | "ToolSeparator",
        ) => (10.5, 500),
        (Look::Material, _) => (10.5, 400),
        (Look::Universal, "TabButton" | "TabBar") => (18.0, 300),
        (Look::Universal, "GroupBox") => (11.25, 600),
        (Look::Universal, _) => (11.25, 400),
    };
    f.point_size = points;
    f.pixel_size = -1;
    f.weight = weight;
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_fields_are_larger_than_its_buttons() {
        let field = font(Look::Material, Some("TextField"));
        let button = font(Look::Material, Some("Button"));
        assert_eq!((field.point_size, field.weight), (12.0, 400));
        assert_eq!((button.point_size, button.weight), (10.5, 500));
        assert_eq!(font(Look::Universal, None).point_size, 11.25);
    }
}
