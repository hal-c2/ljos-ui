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
