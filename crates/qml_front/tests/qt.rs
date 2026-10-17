//! The oracle over the table of types this build has: Qt's where one was
//! generated (`mise run types`), Ljós's own description of its types where
//! not. What holds of both is asserted of both; what the two say
//! differently (a class name, a number for an enum key) is asserted of each.

use qml_front::{QtTypeOracle, Special, TypeOracle};

#[test]
fn the_table_says_what_its_types_have() {
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
    // Ljós's own table has `PropertyChanges` once Ljós has states.
    if let Some(changes) = qt.module_type("QtQuick", "PropertyChanges") {
        assert_eq!(qt.special(changes), Some(Special::PropertyChanges));
    }

    let mouse = qt
        .module_type("QtQuick", "MouseArea")
        .expect("QtQuick has a MouseArea");
    assert!(qt.signal(mouse, "clicked").is_some());
    let keys = qt.module_type("QtQuick", "Keys").expect("QtQuick has Keys");
    assert!(qt.attached(keys).is_some());
    let text = qt
        .module_type("QtQuick", "Text")
        .expect("QtQuick has a Text");
    assert!(qt.has_enum_key(text, None, "AlignHCenter"));
    assert!(!qt.has_module("No.Such.Module"));
}

#[test]
fn a_scoped_key_is_the_key_of_that_enum() {
    let qt = QtTypeOracle;
    // Both of its enums have an `Unknown`; Ljós's own table has the type.
    let network = qt
        .module_type("QtNetwork", "NetworkInformation")
        .expect("QtNetwork has a NetworkInformation");
    assert!(qt.has_enum_key(network, Some("TransportMedium"), "Unknown"));
    assert!(qt.has_enum_key(network, Some("Reachability"), "Unknown"));
    assert_eq!(
        qt.enum_value(network, Some("TransportMedium"), "WiFi"),
        Some(3)
    );
    assert_eq!(
        qt.enum_value(network, Some("Reachability"), "Online"),
        Some(4)
    );
    assert!(!qt.has_enum_key(network, Some("Reachability"), "WiFi"));
}

#[test]
fn the_table_names_the_type_of_a_property_and_the_module_of_a_type() {
    let qt = QtTypeOracle;
    let rectangle = qt.module_type("QtQuick", "Rectangle").unwrap();
    // Qt's table names a value by its class, Ljós's own by its QML name.
    let color = if qt_types::has_qt_table() {
        "QColor"
    } else {
        "color"
    };
    assert_eq!(qt.property_type(rectangle, "color").as_deref(), Some(color));
    assert_eq!(qt.property_type(rectangle, "nothing"), None);
    // A list property keeps its element type; that it is a list is the property's.
    let item = qt.module_type("QtQuick", "Item").unwrap();
    let children = qt.property(item, "children").expect("an Item has children");
    assert!(children.is_list);
    assert!(qt.property_type(item, "children").is_some());
    // `Timer` is QtQml's, whichever module a file imports it by.
    let timer = qt.module_type("QtQuick", "Timer").unwrap();
    assert_eq!(qt.module(timer).as_deref(), Some("QtQml"));
    assert_eq!(qt.module(rectangle).as_deref(), Some("QtQuick"));
}

#[test]
fn the_table_says_when_it_does_not_list_all_the_members_of_a_type() {
    let qt = QtTypeOracle;
    let item = qt.module_type("QtQuick", "Item").unwrap();
    assert!(qt.members_known(item));
    // A row of a model has the roles it is given, whatever they are called.
    let element = qt.module_type("QtQuick", "ListElement").unwrap();
    assert!(!qt.members_known(element));
    let connections = qt.module_type("QtQuick", "Connections").unwrap();
    assert!(!qt.members_known(connections));
}

#[test]
fn the_table_gives_the_number_of_an_enum_key_when_its_description_has_one() {
    let qt = QtTypeOracle;
    let text = qt.module_type("QtQuick", "Text").unwrap();
    // The key is there, and a scope that is not its enum's is no key of
    // that enum at all.
    assert!(qt.has_enum_key(text, None, "AlignHCenter"));
    let number = qt.enum_value(text, None, "AlignHCenter");
    assert_eq!(
        qt.enum_value(text, Some("HAlignment"), "AlignHCenter"),
        number
    );
    assert_eq!(qt.enum_value(text, Some("NotAnEnum"), "AlignHCenter"), None);
    assert_eq!(qt.enum_value(text, None, "NotAKey"), None);
    // A Qt 6 description gives no number for a key; Ljós's types say theirs.
    assert_eq!(number.is_some(), !qt_types::has_qt_table());
}

/// A name no module has is said to be unknown the same way in every build;
/// only a build without Qt's table adds that Qt may have it.
#[test]
fn an_unknown_type_says_what_the_build_cannot_know() {
    let qt = QtTypeOracle;
    let plain = "`NoSuchType` is not a type of anything the file imports";
    let said = qml_front::unknown_type(&qt, "NoSuchType");
    if qt_types::has_qt_table() {
        assert_eq!(said, plain);
    } else {
        assert_eq!(said, format!("{plain} ({})", qt_types::WITHOUT_QT_TABLE));
    }
    // A type some module has is only not imported: nothing Qt's table
    // would add to that.
    assert_eq!(
        qml_front::unknown_type(&qt, "T.Rectangle"),
        "`T.Rectangle` is not a type of anything the file imports"
    );
}
