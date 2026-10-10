//! The oracle over the Qt this machine builds against. Skipped where no type
//! table was generated (`mise run types`).

use qml_front::{QtTypeOracle, Special, TypeOracle};

#[test]
fn qt_says_what_its_types_have() {
    if !qt_types::is_available() {
        eprintln!("no Qt type table: skipped");
        return;
    }
    let qt = QtTypeOracle;
    let rectangle = qt
        .module_type("QtQuick", "Rectangle")
        .expect("QtQuick has a Rectangle");
    assert_eq!(qt.name(rectangle), "Rectangle");
    assert!(qt.property(rectangle, "color").is_some());
    // What every Item has, through the prototype chain.
    assert!(qt.property(rectangle, "width").is_some());
    assert_eq!(qt.default_property(rectangle), Some("data"));
    assert!(qt.property(rectangle, "nothing").is_none());
    assert_eq!(qt.special(rectangle), None);

    let component = qt
        .module_type("QtQml", "Component")
        .expect("QtQml has a Component");
    assert_eq!(qt.special(component), Some(Special::Component));
    let changes = qt
        .module_type("QtQuick", "PropertyChanges")
        .expect("QtQuick has PropertyChanges");
    assert_eq!(qt.special(changes), Some(Special::PropertyChanges));

    let mouse = qt
        .module_type("QtQuick", "MouseArea")
        .expect("QtQuick has a MouseArea");
    assert!(qt.signal(mouse, "clicked").is_some());
    let list = qt
        .module_type("QtQuick", "ListView")
        .expect("QtQuick has a ListView");
    assert!(qt.attached(list).is_some());
    assert!(qt.has_enum_key(list, None, "Horizontal"));
    assert!(!qt.has_module("No.Such.Module"));
}

#[test]
fn qt_names_the_type_of_a_property_and_the_module_of_a_type() {
    if !qt_types::is_available() {
        eprintln!("no Qt type table: skipped");
        return;
    }
    let qt = QtTypeOracle;
    let rectangle = qt.module_type("QtQuick", "Rectangle").unwrap();
    assert_eq!(
        qt.property_type(rectangle, "color").as_deref(),
        Some("QColor")
    );
    assert_eq!(qt.property_type(rectangle, "nothing"), None);
    // A list property keeps its element type; that it is a list is the property's.
    let item = qt.module_type("QtQuick", "Item").unwrap();
    let children = qt.property(item, "children").expect("an Item has children");
    assert!(children.is_list && children.is_readonly);
    assert!(qt.property_type(item, "children").is_some());
    // `Timer` is QtQml's, whichever module a file imports it by.
    let timer = qt.module_type("QtQuick", "Timer").unwrap();
    assert_eq!(qt.module(timer).as_deref(), Some("QtQml"));
    assert_eq!(qt.module(rectangle).as_deref(), Some("QtQuick"));
}

#[test]
fn qt_says_when_it_does_not_list_all_the_members_of_a_type() {
    if !qt_types::is_available() {
        eprintln!("no Qt type table: skipped");
        return;
    }
    let qt = QtTypeOracle;
    let item = qt.module_type("QtQuick", "Item").unwrap();
    assert!(qt.members_known(item));
    // Qt's description of a model is read by a parser of its own.
    let model = qt.module_type("QtQuick", "ListModel").unwrap();
    assert!(!qt.members_known(model));
}

#[test]
fn qt_gives_the_number_of_an_enum_key_when_its_description_has_one() {
    if !qt_types::is_available() {
        eprintln!("no Qt type table: skipped");
        return;
    }
    let qt = QtTypeOracle;
    let list = qt.module_type("QtQuick", "ListView").unwrap();
    // The key is there; a Qt 6 description gives no number for it, and a
    // scope that is not its enum's is no key of that enum at all.
    assert!(qt.has_enum_key(list, None, "Horizontal"));
    let number = qt.enum_value(list, None, "Horizontal");
    assert_eq!(
        qt.enum_value(list, Some("Orientation"), "Horizontal"),
        number
    );
    assert_eq!(qt.enum_value(list, Some("NotAnEnum"), "Horizontal"), None);
    assert_eq!(qt.enum_value(list, None, "NotAKey"), None);
}
