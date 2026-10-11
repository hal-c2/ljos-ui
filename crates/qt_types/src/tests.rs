//! Facts about Qt that the front end leans on, read back from the table.
//!
//! The table is generated locally (`mise run types`), so these skip with a
//! message where there is none, as on CI.

use super::*;

/// Returns from the test when no table was compiled in, or the local Qt the
/// table came from lacks one of the modules the test is about.
macro_rules! require_table {
    ($($module:literal),*) => {
        if !is_available() {
            eprintln!("skipped: no Qt type table; run `mise run types`");
            return;
        }
        $(
            if module($module).is_none() {
                eprintln!("skipped: {} is not installed where the table came from", $module);
                return;
            }
        )*
    };
}

fn ty(module: &str, name: &str) -> &'static Type {
    super::module(module)
        .unwrap_or_else(|| panic!("no module {module}"))
        .type_named(name)
        .unwrap_or_else(|| panic!("no {name} in {module}"))
}

fn property(module: &str, name: &str, property: &str) -> &'static Property {
    ty(module, name)
        .property(property)
        .unwrap_or_else(|| panic!("no {name}.{property}"))
}

#[test]
fn a_module_has_its_own_types() {
    require_table!();
    let rectangle = ty("QtQuick", "Rectangle");
    assert_eq!(
        (rectangle.name, rectangle.class, rectangle.module),
        ("Rectangle", "QQuickRectangle", "QtQuick")
    );
    assert!(rectangle.is_creatable && !rectangle.is_singleton && rectangle.qml_file.is_none());
    assert!(
        super::module("QtQuick")
            .unwrap()
            .type_named("Button")
            .is_none()
    );
    assert!(super::module("QtQuick.NoSuchModule").is_none());
    // A module of Qt.labs is a module like any other.
    if super::module("Qt.labs.qmlmodels").is_none() {
        return;
    }
    let model = ty("Qt.labs.qmlmodels", "TableModel");
    assert_eq!(
        (model.class, model.module),
        ("QQmlTableModel", "Qt.labs.qmlmodels")
    );
    assert!(model.property("rows").is_some() && model.property("columnCount").is_some());
    assert!(
        ty("Qt.labs.qmlmodels", "TableModelColumn")
            .property("display")
            .is_some()
    );
}

/// What a runtime has to have of a module, and what is QML of Qt's own.
#[test]
fn a_module_has_types_in_cpp_and_types_in_qml() {
    require_table!("QtQuick3D.Helpers");
    let native = native_types("QtQuick3D.Helpers").unwrap();
    let written = written_types("QtQuick3D.Helpers").unwrap();
    assert!(native.contains(&"ProceduralMesh") && !native.contains(&"OrbitCameraController"));
    assert!(written.contains(&"OrbitCameraController") && !written.contains(&"ProceduralMesh"));
    assert_eq!(written_types("QtQuick.Layouts"), Some(vec![]));
    assert_eq!(
        (
            native_types("QtQuick.NoSuchModule"),
            written_types("QtQuick.NoSuchModule")
        ),
        (None, None)
    );
}

/// `import QtQuick` is also `QtQml`, `QtQml.Models` and the builtins.
#[test]
fn a_module_has_the_types_of_the_modules_it_imports() {
    require_table!();
    for (name, module) in [
        ("Timer", "QtQml"),
        ("Connections", "QtQml"),
        ("Binding", "QtQml"),
        ("ListModel", "QtQml.Models"),
        ("ListElement", "QtQml.Models"),
        ("QtObject", "QML"),
        ("Component", "QML"),
    ] {
        assert_eq!(ty("QtQuick", name).module, module, "{name}");
    }
    // The import is `QtQuick`'s, not the other way around.
    assert!(
        super::module("QtQml")
            .unwrap()
            .type_named("Rectangle")
            .is_none()
    );
    assert_eq!(ty("QtQuick.Window", "Window").class, "QQuickWindowQmlImpl");
}

#[test]
fn members_are_found_up_the_prototype_chain() {
    require_table!();
    let rectangle = ty("QtQuick", "Rectangle");
    assert_eq!(rectangle.property("color").unwrap().type_name, "QColor");
    assert!(
        rectangle
            .properties
            .iter()
            .all(|property| property.name != "width")
    );
    assert_eq!(rectangle.property("width").unwrap().type_name, "double");
    assert_eq!(rectangle.prototype().unwrap().name, "Item");
    assert_eq!(
        rectangle.prototype().unwrap().prototype().unwrap().name,
        "QtObject"
    );
    assert!(rectangle.property("objectName").is_some());
    assert!(rectangle.property("colour").is_none());
    assert!(rectangle.has_method("forceActiveFocus") && !rectangle.has_method("start"));
    assert!(ty("QtQuick", "Timer").has_method("restart"));
}

#[test]
fn a_property_knows_what_it_holds() {
    require_table!();
    let delegate = property("QtQuick", "ListView", "delegate");
    assert!(delegate.is_component && delegate.is_pointer && !delegate.is_list);
    assert!(property("QtQuick", "Loader", "sourceComponent").is_component);
    assert!(!property("QtQuick", "ListView", "model").is_component);

    let data = property("QtQuick", "Item", "data");
    assert!(data.is_list && data.is_readonly);
    assert_eq!(data.value_type().unwrap().name, "QtObject");
    let states = property("QtQuick", "Item", "states");
    assert!(states.is_list);
    assert_eq!(states.value_type().unwrap().name, "State");
    assert!(!property("QtQuick", "Item", "width").is_readonly);
}

/// What Qt answers: with `QtQuick.VectorImage` imported `ItemSpy` is a type
/// and `Shape` is not, though its qmldir names both modules.
#[test]
fn a_default_import_is_followed_where_there_is_a_choice() {
    require_table!();
    let module = |uri| super::module(uri).unwrap();
    assert_eq!(
        ty("QtQuick.Controls", "Button").module,
        "QtQuick.Controls.Basic"
    );
    assert_eq!(
        ty("QtQuick.VectorImage", "ItemSpy").module,
        "QtQuick.VectorImage.Helpers"
    );
    assert!(module("QtQuick.VectorImage").type_named("Shape").is_none());
    assert!(
        module("QtQuick.VectorImage")
            .type_named("MultiEffect")
            .is_none()
    );
}

/// QtCharts and QtGraphs each have a `QAbstractAxis`, and they are not one
/// class: an axis of a graph has a delegate for its labels, one of a chart
/// has a font for them.
#[test]
fn classes_of_one_name_are_each_their_modules() {
    require_table!("QtGraphs", "QtCharts");
    assert!(property("QtGraphs", "ValueAxis", "labelDelegate").is_component);
    assert!(ty("QtGraphs", "ValueAxis").property("labelsFont").is_none());
    assert_eq!(
        property("QtCharts", "ValueAxis", "labelsFont").type_name,
        "QFont"
    );
    assert!(
        ty("QtCharts", "ValueAxis")
            .property("labelDelegate")
            .is_none()
    );
    assert_eq!(
        property("QtGraphs", "GraphsView", "axisX").type_name,
        "QAbstractAxis@QtGraphs"
    );
    assert_eq!(
        property("QtCharts", "LineSeries", "axisX").type_name,
        "QAbstractAxis@QtCharts"
    );
}

#[test]
fn the_default_property_is_inherited() {
    require_table!();
    assert_eq!(ty("QtQuick", "Item").default_property(), Some("data"));
    assert_eq!(ty("QtQuick", "Rectangle").default_property(), Some("data"));
    assert_eq!(ty("QtQuick", "QtObject").default_property(), None);
    assert_eq!(
        ty("QtQuick.Controls", "ApplicationWindow").default_property(),
        Some("contentData")
    );
}

#[test]
fn a_signal_names_its_parameters() {
    require_table!();
    let mouse_area = ty("QtQuick", "MouseArea");
    assert_eq!(mouse_area.signal("clicked").unwrap().parameters, ["mouse"]);
    assert_eq!(mouse_area.signal("wheel").unwrap().parameters, ["wheel"]);
    assert!(
        mouse_area
            .signal("widthChanged")
            .unwrap()
            .parameters
            .is_empty()
    );
    assert!(mouse_area.signal("tapped").is_none());
    assert!(ty("QtQuick", "Timer").signal("triggered").is_some());
    // Qt's header leaves this one unnamed.
    assert_eq!(
        ty("QtQuick", "Item")
            .signal("childrenRectChanged")
            .unwrap()
            .parameters,
        [""]
    );
}

/// Qt 6 describes an enum by its keys alone, so there is no number to give.
#[test]
fn an_enum_key_is_found_on_the_type() {
    require_table!();
    let raised = ty("QtQuick", "Text").enum_value("Raised").unwrap();
    assert_eq!(
        (raised.enumeration.name, raised.key, raised.number),
        ("TextStyle", "Raised", None)
    );
    assert_eq!(
        ty("QtQuick", "ListView")
            .enum_value("Horizontal")
            .unwrap()
            .enumeration
            .name,
        "Orientation"
    );
    assert_eq!(
        ty("QtQuick", "Image")
            .enum_value("PreserveAspectFit")
            .unwrap()
            .enumeration
            .name,
        "FillMode"
    );
    assert!(ty("QtQuick", "Text").enum_value("Sideways").is_none());
}

/// `Qt.AlignLeft`: the singleton has its enums from a namespace extension.
#[test]
fn the_qt_singleton_has_the_qt_namespace() {
    require_table!();
    let qt = ty("QtQuick", "Qt");
    assert!(qt.is_singleton && !qt.is_creatable);
    let left = qt.enum_value("AlignLeft").unwrap();
    assert!(left.enumeration.is_flag);
    assert_eq!(
        (left.enumeration.name, left.enumeration.alias),
        ("Alignment", Some("AlignmentFlag"))
    );
    assert!(qt.enum_value("Key_Return").is_some());
    assert!(qt.has_method("rgba") && qt.has_method("openUrlExternally"));
}

/// A property declared `Qt::Alignment` holds several keys; one declared
/// with the enum the flags are made of, or with a plain enum, holds one.
#[test]
fn flags_are_told_from_the_enum_of_one_key() {
    require_table!();
    let qt = ty("QtQuick", "Qt");
    assert_eq!(qt.enum_is_flags("Alignment"), Some(true));
    assert_eq!(qt.enum_is_flags("AlignmentFlag"), Some(false));
    assert_eq!(qt.enum_is_flags("MouseButtons"), Some(true));
    assert_eq!(qt.enum_is_flags("Orientation"), Some(false));
    assert_eq!(qt.enum_is_flags("Nowhere"), None);
    let text = ty("QtQuick", "Text");
    assert_eq!(text.enum_is_flags("HAlignment"), Some(false));
    // Inherited from Item.
    assert_eq!(text.enum_is_flags("TransformOrigin"), Some(false));
    let popup = ty("QtQuick.Controls", "Popup");
    assert_eq!(popup.enum_is_flags("ClosePolicy"), Some(true));
}

#[test]
fn an_attached_type_has_its_own_members() {
    require_table!();
    let layout = ty("QtQuick.Layouts", "Layout");
    assert!(!layout.is_creatable);
    let attached = layout.attached().unwrap();
    assert_eq!(attached.class, "QQuickLayoutAttached");
    assert_eq!(attached.property("fillWidth").unwrap().type_name, "bool");
    // `RowLayout` is a `Layout`, so it attaches the same; `Item` does not.
    assert_eq!(
        ty("QtQuick.Layouts", "RowLayout").attached().unwrap().class,
        attached.class
    );
    assert!(ty("QtQuick", "Item").attached().is_none());

    let keys = ty("QtQuick", "Keys").attached().unwrap();
    assert_eq!(keys.signal("pressed").unwrap().parameters, ["event"]);
    assert!(
        ty("QtQuick", "Component")
            .attached()
            .unwrap()
            .signal("completed")
            .is_some()
    );
    assert!(
        ty("QtQuick", "ListView")
            .attached()
            .unwrap()
            .property("isCurrentItem")
            .is_some()
    );
}

/// `font.bold`, `anchors.fill`, `border.color`.
#[test]
fn a_grouped_property_is_walked_through_its_type() {
    require_table!();
    let font = property("QtQuick", "Text", "font").value_type().unwrap();
    assert!(font.is_value && !font.is_creatable);
    assert_eq!((font.name, font.class), ("font", "QFont"));
    assert_eq!(font.property("bold").unwrap().type_name, "bool");
    assert_eq!(font.property("pixelSize").unwrap().type_name, "int");

    let anchors = property("QtQuick", "Item", "anchors").value_type().unwrap();
    assert_eq!(
        anchors.property("fill").unwrap().value_type().unwrap().name,
        "Item"
    );
    let border = property("QtQuick", "Rectangle", "border")
        .value_type()
        .unwrap();
    assert_eq!(
        border.property("color").unwrap().value_type().unwrap().name,
        "color"
    );
}

/// `QtQuick.Controls` has no types of its own: its qmldir says
/// `default import QtQuick.Controls.Basic` and `optional import` for the
/// other styles, one of which takes Basic's place when chosen at run time.
#[test]
fn controls_are_those_of_the_default_style() {
    require_table!();
    let controls = super::module("QtQuick.Controls").unwrap();
    let kind = |uri| {
        controls
            .imports
            .iter()
            .find(|import| import.module == uri)
            .map(|import| import.kind)
    };
    assert_eq!(kind("QtQuick.Controls.Basic"), Some(ImportKind::Default));
    assert_eq!(
        kind("QtQuick.Controls.Material"),
        Some(ImportKind::Optional)
    );
    assert_eq!(kind("QtQuick.Controls.Fusion"), Some(ImportKind::Optional));

    let button = ty("QtQuick.Controls", "Button");
    assert_eq!(button.qml_file, Some("QtQuick/Controls/Basic/Button.qml"));
    assert_eq!(
        (button.name, button.module),
        ("Button", "QtQuick.Controls.Basic")
    );
    assert!(button.is_creatable);
    // The file's root is the template, which is where the members are.
    assert_eq!(button.prototype().unwrap().class, "QQuickButton");
    assert_eq!(button.property("text").unwrap().type_name, "QString");
    assert_eq!(
        button
            .property("background")
            .unwrap()
            .value_type()
            .unwrap()
            .name,
        "Item"
    );
    assert!(button.signal("clicked").is_some());
    assert_eq!(button.default_property(), Some("data"));

    // A style named outright is that style.
    let material = ty("QtQuick.Controls.Material", "Button");
    assert_eq!(
        material.qml_file,
        Some("QtQuick/Controls/Material/Button.qml")
    );
    assert_eq!(material.prototype().unwrap().class, "QQuickButton");
    // What a style does not write in QML is C++ and the same in every style.
    assert_eq!(ty("QtQuick.Controls", "Overlay").class, "QQuickOverlay");
    assert!(ty("QtQuick.Controls", "Overlay").qml_file.is_none());
}

#[test]
fn every_link_in_the_table_is_to_a_type() {
    require_table!();
    let table = table();
    assert!(table.modules.len() > 20 && table.types.len() > 500);
    for uri in table.modules.keys() {
        let module = super::module(uri).unwrap();
        for import in &module.imports {
            // An optional style may not be in the table; any other must be.
            assert!(
                import.kind == ImportKind::Optional || super::module(import.module).is_some(),
                "{uri} imports {}",
                import.module
            );
        }
    }
}

/// Without a table nothing is known, and nothing panics.
#[test]
fn an_absent_table_finds_nothing() {
    if is_available() {
        assert!(module("QtQuick").is_some());
        return;
    }
    assert!(modules().is_empty());
    assert!(module("QtQuick").is_none());
    assert_eq!(native_types("QtQuick"), None);
    assert!(!is_member_name("width"));
}

/// The listing is every module `module` finds, once, in order.
#[test]
fn modules_lists_every_module_in_order() {
    require_table!("QtQuick");
    let all = modules();
    assert!(
        all.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted, unique"
    );
    assert!(all.contains(&"QtQuick"));
    assert!(all.iter().all(|uri| module(uri).is_some()));
}

/// Ljós's own module is read next to Qt's: its types inherit Qt's and have
/// the members its native types declare.
#[test]
fn the_ljos_module_is_in_the_table() {
    require_table!("QtQuick");
    let boundary = ty("Ljos", "ErrorBoundary");
    assert!(boundary.is_creatable);
    assert_eq!(boundary.default_property(), Some("content"));
    assert!(boundary.property("content").unwrap().is_component);
    assert!(boundary.property("fallback").unwrap().is_component);
    assert!(boundary.property("width").is_some(), "an Item");
    assert!(boundary.has_method("retry"));
    let fault = boundary.property("fault").unwrap();
    assert!(fault.is_readonly);
    let fault = fault.value_type().expect("the fault's members");
    assert!(fault.property("kind").is_some() && fault.property("line").is_some());
    assert!(
        module("QtQuick")
            .unwrap()
            .type_named("ErrorBoundary")
            .is_none()
    );
}

/// `QtTest` is a module of native types too, described as `Ljos` is.
#[test]
fn the_qttest_module_is_in_the_table() {
    require_table!("QtQuick");
    let case = ty("QtTest", "TestCase");
    assert!(case.is_creatable);
    assert_eq!((case.name, case.module), ("TestCase", "QtTest"));
    assert_eq!(case.prototype().map(|item| item.class), Some("QQuickItem"));
    assert_eq!(case.default_property(), Some("data"), "an Item's");
    assert!(case.has_method("compare") && !case.has_method("clear"));
    // A type is named as QML names it and known by Qt's class.
    assert_eq!(property("QtTest", "TestCase", "name").type_name, "QString");
    assert!(case.signal("whenChanged").is_some());
    let spy = ty("QtTest", "SignalSpy");
    assert!(spy.has_method("clear") && spy.signal("countChanged").is_some());
    assert!(property("QtTest", "SignalSpy", "count").is_readonly);
    let target = property("QtTest", "SignalSpy", "target");
    assert!(target.is_pointer && !target.is_readonly);
    assert_eq!(
        target.value_type().map(|object| object.class),
        Some("QObject")
    );
    // What a test is handed, and cannot make.
    assert!(!ty("QtTest", "TouchEventSequence").is_creatable);
    // What the module's types are written with is not the module's.
    assert!(module("QtTest").unwrap().type_named("Item").is_none());
    assert!(module("QtQuick").unwrap().type_named("TestCase").is_none());
}

/// The own modules name types as QML does. Every such name has to be one
/// Qt's table or the module itself has, or a member is of no known type.
#[test]
fn every_name_in_the_own_modules_is_a_type() {
    require_table!("QtQuick");
    let own = read(&[OWN]);
    assert!(own.modules.contains_key("Ljos") && own.modules.contains_key("QtTest"));
    for uri in own.modules.keys() {
        let exported = native_types(uri).expect("an own module is in the table");
        assert!(!exported.is_empty(), "{uri}");
        for name in exported {
            assert!(ty(uri, name).prototype().is_some(), "what {name} inherits");
        }
    }
    for described in &table().types {
        if !own.modules.contains_key(described.module) {
            continue;
        }
        for property in &described.properties {
            assert!(
                property.value_type().is_some()
                    || described.enum_is_flags(property.type_name).is_some(),
                "the type of {}.{}: {}",
                described.class,
                property.name,
                property.type_name
            );
        }
    }
}

/// A table in which a module describes its types by QML names.
const NAMED: &str = "\
module Deep
\tstring QString
\ttype QString string
module Base
\timport Deep
\tItem QItem
\tComponent QQmlComponent
\tPart QPart
\ttype QItem Item
\ttype QQmlComponent Component
\ttype QPart Part
module Own
\tuses Base
\tThing Own.Thing
\tPart Own.Part
\ttype Own.Thing Thing
\t\tprototype Item named
\t\tprop label string named
\t\tprop template Component named pointer
\t\tprop part Part named pointer
\t\tprop lost Nothing named
\t\tprop plain QItem pointer
\ttype Own.Part Part
\t\tprototype Item
";

fn class_of(table: &Table, index: Option<usize>) -> Option<&'static str> {
    index.map(|index| table.types[index].class)
}

#[test]
fn a_named_type_is_found_as_a_file_that_imports_what_the_module_uses_would() {
    let table = read(&[NAMED]);
    let thing = &table.types[table.classes["Own.Thing"]];
    assert_eq!(class_of(&table, thing.prototype), Some("QItem"));
    let type_of = |name: &str| {
        let property = thing.properties.iter().find(|p| p.name == name).unwrap();
        (property.type_name, property.is_component)
    };
    // Through what the used module imports, as a file would find it.
    assert_eq!(type_of("label"), ("QString", false));
    assert_eq!(type_of("template"), ("QQmlComponent", true));
    // The module's own type before another's of the same name.
    assert_eq!(type_of("part"), ("Own.Part", false));
    // A name nothing has stays what was written.
    assert_eq!(type_of("lost"), ("Nothing", false));
}

#[test]
fn a_type_that_is_not_named_is_a_class() {
    let table = read(&[NAMED]);
    let thing = &table.types[table.classes["Own.Thing"]];
    let plain = thing.properties.iter().find(|p| p.name == "plain").unwrap();
    assert_eq!(plain.type_name, "QItem");
    // `Item` is a QML name: no class is called that.
    let part = &table.types[table.classes["Own.Part"]];
    assert_eq!(part.prototype, None);
}

#[test]
fn what_a_module_uses_is_not_its_own() {
    let table = read(&[NAMED]);
    let find =
        |uri: &str, name: &str| class_of(&table, table.type_named(uri, name, &mut Vec::new()));
    assert_eq!(find("Own", "Thing"), Some("Own.Thing"));
    assert_eq!(find("Own", "Item"), None);
    assert_eq!(find("Own", "string"), None);
    // An import is: Deep's `string` is Base's too.
    assert_eq!(find("Base", "string"), Some("QString"));
}
