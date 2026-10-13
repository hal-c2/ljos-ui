//! Every property that holds an enum says so.
//!
//! A property tagged with its enum takes a key by name (`elide: "ElideRight"`,
//! `acceptedButtons = "LeftButton|RightButton"`) and refuses a string that
//! names none; one left as a plain number takes neither. Nothing fails to
//! compile when a tag is forgotten, so this goes over every registered type:
//! what Qt's type description declares with an enum is tagged here, and as
//! flags where Qt's holds several keys.

use ljos_engine::{Engine, MetaRef, TypeTag};
use qml_front::{QtType, QtTypeOracle, TypeOracle};

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e
}

/// Every registered type and what it attaches, with the name to report.
fn metas(e: &Engine) -> Vec<(String, String, String, MetaRef)> {
    let mut out = Vec::new();
    for module in &e.type_dump().modules {
        for ty in &module.types {
            let meta = e
                .lookup_type(&module.uri, &ty.name)
                .expect("a dumped type is registered");
            if let Some(attached) = meta.attached {
                let name = format!("{} (attached)", ty.name);
                out.push((module.uri.clone(), ty.name.clone(), name, attached()));
            }
            out.push((module.uri.clone(), ty.name.clone(), ty.name.clone(), meta));
        }
    }
    out
}

/// A tag names an enum the property's object can look a key up in: without
/// it every string is refused, the right keys too.
#[test]
fn a_tagged_property_names_an_enum_of_its_type() {
    let e = engine();
    let mut wrong = Vec::new();
    for (_, _, name, meta) in metas(&e) {
        for property in &meta.properties {
            let (TypeTag::Enum(held) | TypeTag::Flags(held)) = &property.ty else {
                continue;
            };
            if !meta.has_enumeration(held.as_str()) {
                wrong.push(format!("{name}.{}: no enum {held}", property.name));
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Enums Qt declares on a class that is no QML type, by the name its
/// description gives a property: whether a property of it holds several
/// keys.
const FOREIGN: &[(&str, bool)] = &[
    ("QPlatformDialogHelper::StandardButtons", true),
    ("QPlatformDialogHelper::ButtonLayout", false),
    ("QPlatformDialogHelper::ButtonRole", false),
    ("QAccessible::Role", false),
    ("QWindow::Visibility", false),
    // Declared on the type, read through what it attaches.
    ("QQuickTabBar::Position", false),
    ("QQuickStackView::Status", false),
    ("QQuickDrag::DragType", false),
    // QtQuick.Dialogs: the options are declared on classes of QtGui.
    ("QColorDialogOptions::ColorDialogOptions", true),
    ("QFileDialogOptions::FileDialogOptions", true),
    ("QFontDialogOptions::FontDialogOptions", true),
    ("QQuickSystemPalette::ColorGroup", false),
    // QtCore's permissions: the enums are on the C++ permission classes.
    ("QLocationPermission::Accuracy", false),
    ("QLocationPermission::Availability", false),
    ("QCalendarPermission::AccessMode", false),
    ("QContactsPermission::AccessMode", false),
    ("QBluetoothPermission::CommunicationModes", true),
];

/// Enums of a type Ljós does not have yet (`PointerDevice`): what is
/// declared with them stays a plain number, since no key could be looked
/// up. Remove an entry when its type is registered.
const NOT_YET: &[&str] = &["QInputDevice::DeviceTypes", "QPointingDevice::PointerTypes"];

/// Whether Qt declares a property of `owner` with an enum, and if so
/// whether with flags.
fn qt_enum(owner: QtType, declared: &str) -> Option<bool> {
    if let Some(of_qt) = declared.strip_prefix("Qt::") {
        let qt = QtTypeOracle.module_type("QtQuick", "Qt")?;
        return qt.0.enum_is_flags(of_qt);
    }
    if let Some((_, flags)) = FOREIGN.iter().find(|(name, _)| *name == declared) {
        return Some(*flags);
    }
    owner.0.enum_is_flags(declared)
}

#[test]
fn what_qt_declares_with_an_enum_is_tagged_with_one() {
    // Without Qt's description there is nothing to hold the tags to: the
    // table is then written from these very tags.
    if !qt_types::test_has_qt_table("what_qt_declares_with_an_enum_is_tagged_with_one") {
        return;
    }
    let e = engine();
    let mut wrong = Vec::new();
    let mut seen = 0;
    for (module, ty, name, meta) in metas(&e) {
        let Some(qt) = QtTypeOracle.module_type(&module, &ty) else {
            continue;
        };
        let qt = if name == ty {
            qt
        } else {
            match qt.0.attached() {
                Some(attached) => QtType(attached),
                None => continue,
            }
        };
        for property in &meta.properties {
            let Some(declared) = qt.0.property(&property.name) else {
                continue;
            };
            if declared.is_list
                || declared.value_type().is_some()
                || NOT_YET.contains(&declared.type_name)
            {
                continue;
            }
            let Some(flags) = qt_enum(qt, declared.type_name) else {
                // An enum of a class this file has not heard of would be
                // passed over as if it were no enum.
                if declared.type_name.contains("::") && !declared.is_pointer {
                    wrong.push(format!(
                        "{name}.{}: is {} an enum? List it in FOREIGN",
                        property.name, declared.type_name
                    ));
                }
                continue;
            };
            seen += 1;
            let tagged = match &property.ty {
                TypeTag::Enum(_) => Some(false),
                TypeTag::Flags(_) => Some(true),
                _ => None,
            };
            if tagged != Some(flags) {
                wrong.push(format!(
                    "{name}.{}: Qt has {} {}, here it is {}",
                    property.name,
                    if flags { "flags" } else { "enum" },
                    declared.type_name,
                    property.ty.name()
                ));
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // A table that stopped matching anything would pass the same way.
    assert!(seen > 150, "{seen} enum properties were compared");
}
