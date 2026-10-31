//! Facts the front end leans on, read back from the table.
//!
//! Three kinds of test: of Qt's own description (they are ignored, with the
//! reason, in a build that has no table of Qt's types), of Ljós's own table
//! read alone (the same in every build), and of the reader itself on small
//! tables written here.

use super::*;

/// False, having said so on the terminal, when the local Qt the table came
/// from was installed without one of `modules`.
fn installed(test: &str, modules: &[&str]) -> bool {
    let missing = modules.iter().find(|uri| module(uri).is_none());
    if let Some(uri) = missing {
        use std::io::Write as _;
        // Past what the harness captures of a passing test.
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPED {test}: {uri} is not installed where the table of Qt's types came from"
        );
    }
    missing.is_none()
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_module_has_its_own_types() {
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
    if !installed("a_module_has_its_own_types", &["Qt.labs.qmlmodels"]) {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_module_has_types_in_cpp_and_types_in_qml() {
    if !installed(
        "a_module_has_types_in_cpp_and_types_in_qml",
        &["QtQuick3D.Helpers"],
    ) {
        return;
    }
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_module_has_the_types_of_the_modules_it_imports() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn members_are_found_up_the_prototype_chain() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_property_knows_what_it_holds() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_default_import_is_followed_where_there_is_a_choice() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn classes_of_one_name_are_each_their_modules() {
    if !installed(
        "classes_of_one_name_are_each_their_modules",
        &["QtGraphs", "QtCharts"],
    ) {
        return;
    }
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn the_default_property_is_inherited() {
    assert_eq!(ty("QtQuick", "Item").default_property(), Some("data"));
    assert_eq!(ty("QtQuick", "Rectangle").default_property(), Some("data"));
    assert_eq!(ty("QtQuick", "QtObject").default_property(), None);
    assert_eq!(
        ty("QtQuick.Controls", "ApplicationWindow").default_property(),
        Some("contentData")
    );
}

#[test]
fn a_signal_says_the_type_of_each_parameter() {
    let table = read(
        "",
        "module M\n\tT M.T\n\ttype M.T T\n\t\tsignal done a:real _:Qt::Key c\n",
    );
    let index = table.modules["M"].exports["T"];
    let signal = &table.types[index].signals[0];
    assert_eq!(signal.parameters, ["a", "", "c"]);
    assert_eq!(signal.parameter_types, ["real", "Qt::Key", ""]);
}

#[test]
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_signal_names_its_parameters() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn an_enum_key_is_found_on_the_type() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn the_qt_singleton_has_the_qt_namespace() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn flags_are_told_from_the_enum_of_one_key() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn an_attached_type_has_its_own_members() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn a_grouped_property_is_walked_through_its_type() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn controls_are_those_of_the_default_style() {
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
#[cfg_attr(
    not(qt_types_table),
    ignore = "about Qt's own description of its types, and this build has no table of them"
)]
fn every_link_in_the_table_is_to_a_type() {
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

/// Which description a module is read from is one answer per build.
#[test]
fn a_module_is_described_by_one_table() {
    for (uri, module) in &table().modules {
        // Qt has no `Ljos`; everything else is Qt's wherever Qt's table is.
        let own = !has_qt_table() || *uri == "Ljos";
        assert!(!own || module.is_own, "{uri}");
    }
    for ty in &table().types {
        assert_eq!(ty.is_own, module(ty.module).unwrap().is_own, "{}", ty.class);
    }
    assert!(module("QtQuick").is_some(), "in either table");
    assert_eq!(module("QtQuick").unwrap().is_own, !has_qt_table());
    assert!(module("QtQuick.NoSuchModule").is_none());
}

/// What the runtime adds to one of Qt's types is known by name where Qt's
/// table describes the type; without it, Ljós's own table lists it.
#[test]
fn ljos_adds_members_to_qt_types() {
    let quick = module("QtQuick").unwrap();
    let font = quick.type_named("font").unwrap();
    let area = quick.type_named("MouseArea").unwrap();
    let accessible = quick.type_named("Accessible").unwrap().attached().unwrap();
    let component = module("QtQml").unwrap().type_named("Component").unwrap();
    let component = component.attached().unwrap();
    for (ty, name) in [
        (font, "scaleWithSystem"),
        (accessible, "showOnScreenAction"),
        (component, "reloaded"),
    ] {
        let listed = ty.property(name).is_some() || ty.signal(name).is_some();
        assert_eq!(
            ty.adds(name),
            has_qt_table() && !listed,
            "{}.{name}",
            ty.name
        );
        assert!(listed || ty.adds(name), "{}.{name}", ty.name);
    }
    // Inherited, as what Ljós adds to `Item` is a `MouseArea`'s too.
    assert!(!area.adds("widht"));
    assert!(!font.adds("sparkle"));
}

/// A style's part that Qt's table names but does not describe has what the
/// type Ljós's own file builds it on has: a `CheckIndicator` is a
/// `Rectangle`, so `height` is its own.
#[test]
fn an_undescribed_qml_file_has_what_ljos_builds_it_on() {
    let indicator = ty("QtQuick.Controls.Fusion.impl", "CheckIndicator");
    if indicator.is_own {
        assert!(indicator.property("height").is_some());
        return;
    }
    if indicator.prototype().is_some() {
        // Described by this Qt; nothing stands in.
        return;
    }
    for name in ["height", "color", "control", "childrenRectChanged"] {
        assert!(indicator.adds(name), "{name}");
    }
    assert!(!indicator.adds("widht"));
}

/// A part Ljós's own table lists is not unlisted; one only Qt's table
/// names (`CopyAction`, a file of Qt's Fusion that Ljós does not have) is.
#[test]
fn a_part_only_qt_names_is_unlisted() {
    assert!(!ty("QtQuick.Controls.Fusion.impl", "CheckIndicator").is_unlisted);
    let copy = module("QtQuick.Controls.Fusion.impl").and_then(|m| m.type_named("CopyAction"));
    assert_eq!(copy.is_some(), has_qt_table());
    if let Some(copy) = copy
        && copy.prototype().is_none()
    {
        assert!(copy.is_unlisted);
    }
    // Nor is anything of a module Ljós's own table does not describe.
    if let Some(charts) = module("QtCharts") {
        assert!(
            !charts
                .type_named("ChartView")
                .is_some_and(|ty| ty.is_unlisted)
        );
    }
}

/// The listing is every module `module` finds, once, in order.
#[test]
fn modules_lists_every_module_in_order() {
    let all = modules();
    assert!(
        all.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted, unique"
    );
    assert!(all.contains(&"QtQuick") && all.contains(&"Ljos"));
    assert!(all.iter().all(|uri| module(uri).is_some()));
}

#[test]
fn a_name_some_module_has_is_a_type_name() {
    assert!(is_type_name("Rectangle") && is_type_name("ErrorBoundary"));
    assert!(!is_type_name("NoSuchType") && !is_type_name("width"));
    assert!(is_member_name("width") && !is_member_name("noSuchMember"));
}

/// Ljós's own module is in the table of either build: its types are
/// `Item`s, whichever table says what an `Item` is.
#[test]
fn the_ljos_module_is_in_the_table() {
    let boundary = ty("Ljos", "ErrorBoundary");
    assert!(boundary.is_creatable && boundary.is_own);
    assert_eq!(boundary.default_property(), Some("content"));
    assert!(boundary.property("content").unwrap().is_component);
    assert!(boundary.property("fallback").unwrap().is_component);
    assert!(boundary.property("width").is_some(), "an Item");
    assert_eq!(boundary.prototype().map(|item| item.name), Some("Item"));
    assert_eq!(
        boundary.prototype().map(|item| item.is_own),
        Some(!has_qt_table())
    );
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

/// `QtTest` is a module of native types too. Qt's table describes it only
/// where the local Qt has it, and then Qt's description is the one read.
#[test]
fn the_qttest_module_is_in_the_table() {
    let case = ty("QtTest", "TestCase");
    assert!(case.is_creatable);
    assert_eq!((case.name, case.module), ("TestCase", "QtTest"));
    assert!(case.property("width").is_some(), "an Item");
    assert_eq!(case.default_property(), Some("data"), "an Item's");
    assert!(case.has_method("compare") && !case.has_method("clear"));
    assert!(class::STRING.contains(&property("QtTest", "TestCase", "name").type_name));
    assert!(case.signal("whenChanged").is_some());
    let spy = ty("QtTest", "SignalSpy");
    assert!(spy.has_method("clear") && spy.signal("countChanged").is_some());
    assert!(property("QtTest", "SignalSpy", "count").is_readonly);
    let target = property("QtTest", "SignalSpy", "target");
    assert!(target.is_pointer && !target.is_readonly);
    assert_eq!(
        target.value_type().map(|object| object.name),
        Some("QtObject")
    );
    // What the module's types are written with is not the module's.
    assert!(module("QtTest").unwrap().type_named("Item").is_none());
    assert!(module("QtQuick").unwrap().type_named("TestCase").is_none());
}

/// Ljós's own table read with no other, as a build without Qt's has it:
/// the same in every build, so a machine that has Qt's table sees here what
/// one without it is given.
fn own() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| read("", OWN))
}

#[test]
fn the_table_keeps_no_room_it_grew_while_read() {
    let own = own();
    assert_eq!(own.types.capacity(), own.types.len());
    for ty in &own.types {
        assert_eq!(
            ty.properties.capacity(),
            ty.properties.len(),
            "{}",
            ty.class
        );
        assert_eq!(ty.enums.capacity(), ty.enums.len(), "{}", ty.class);
        assert_eq!(ty.signals.capacity(), ty.signals.len(), "{}", ty.class);
        assert_eq!(ty.methods.capacity(), ty.methods.len(), "{}", ty.class);
    }
}

fn own_type(uri: &str, name: &str) -> &'static Type {
    let own = own();
    let index = own
        .type_named(uri, name, &mut Vec::new())
        .unwrap_or_else(|| panic!("no {name} in {uri} of Ljós's own table"));
    &own.types[index]
}

/// The classes from a type up what it inherits, in a table read here (the
/// lookups of [`Type`] are of the build's table).
fn chain(table: &'static Table, ty: &'static Type) -> Vec<&'static Type> {
    let mut chain = vec![ty];
    while let Some(index) = chain.last().unwrap().prototype {
        chain.push(&table.types[index]);
    }
    chain
}

fn own_property(ty: &'static Type, name: &str) -> Option<&'static Property> {
    chain(own(), ty)
        .into_iter()
        .find_map(|ty| ty.properties.iter().find(|property| property.name == name))
}

#[test]
fn the_own_table_alone_has_the_modules_ljos_implements() {
    let own = own();
    for uri in [
        "Ljos",
        "QtQml",
        "QtQml.Models",
        "QtQuick",
        "QtQuick.Window",
        "QtQuick.Layouts",
        "QtQuick.Templates",
        "QtQuick.Controls",
        "QtQuick.Controls.Basic",
        "QtTest",
    ] {
        let module = own.modules.get(uri).unwrap_or_else(|| panic!("{uri}"));
        assert!(module.is_own && !module.exports.is_empty(), "{uri}");
    }
    assert!(own.types.iter().all(|ty| ty.is_own));
}

#[test]
fn the_own_table_alone_says_what_a_rectangle_is() {
    let rectangle = own_type("QtQuick", "Rectangle");
    assert_eq!(
        (rectangle.name, rectangle.class, rectangle.module),
        ("Rectangle", "QtQuick.Rectangle", "QtQuick")
    );
    assert!(rectangle.is_creatable && !rectangle.is_singleton && rectangle.qml_file.is_none());
    let classes: Vec<&str> = chain(own(), rectangle).iter().map(|ty| ty.class).collect();
    assert_eq!(
        classes,
        ["QtQuick.Rectangle", "QtQuick.Item", "QtQml.QtObject"]
    );
    let color = own_property(rectangle, "color").unwrap();
    assert_eq!(color.type_name, "QtQuick.color");
    assert!(own().types[own().classes[color.type_name]].is_value);
    assert!(own_property(rectangle, "width").is_some(), "an Item's");
    assert!(own_property(rectangle, "objectName").is_some());
    assert!(own_property(rectangle, "colour").is_none());
    let item = own_type("QtQuick", "Item");
    assert_eq!(item.default_property, Some("data"));
    assert!(item.enums.iter().any(|e| e.name == "TransformOrigin"));
    // A number for every key: Ljós's types say theirs.
    assert!(
        item.enums
            .iter()
            .all(|e| e.keys.iter().all(|(_, n)| n.is_some()))
    );
}

/// `import QtQuick` is also `QtQml` and its models; not the other way.
#[test]
fn the_own_table_alone_follows_what_a_module_comes_with() {
    for (name, module) in [
        ("Timer", "QtQml"),
        ("QtObject", "QtQml"),
        ("Component", "QtQml"),
        ("Connections", "QtQml"),
        ("ListModel", "QtQml.Models"),
    ] {
        assert_eq!(own_type("QtQuick", name).module, module, "{name}");
        assert_eq!(own_type("QtQuick.Window", name).module, module, "{name}");
    }
    let find = |uri: &str, name: &str| own().type_named(uri, name, &mut Vec::new());
    assert!(find("QtQml", "Item").is_none());
    assert!(find("QtQuick", "Button").is_none());
    assert!(find("QtQuick.Controls", "Item").is_none());
    assert!(find("QtQuick.Layouts", "Item").is_none());
}

/// A control is the QML file of the style, over the template that has the
/// members.
#[test]
fn the_own_table_alone_has_the_style_s_files() {
    let button = own_type("QtQuick.Controls", "Button");
    assert_eq!(button.qml_file, Some("QtQuick/Controls/Basic/Button.qml"));
    assert_eq!(button.module, "QtQuick.Controls.Basic");
    assert!(button.is_creatable);
    let classes: Vec<&str> = chain(own(), button).iter().map(|ty| ty.class).collect();
    assert_eq!(classes[1], "QtQuick.Templates.Button");
    assert!(classes.contains(&"QtQuick.Templates.AbstractButton"));
    assert!(classes.contains(&"QtQuick.Item"));
    assert!(own_property(button, "text").is_some());
    assert_eq!(
        own_type("QtQuick.Controls.Basic", "Button").class,
        button.class,
        "one type in both modules"
    );
}

#[test]
fn the_own_table_alone_has_the_values_and_the_namespace() {
    let font = own_type("QtQuick", "font");
    assert!(font.is_value && !font.is_creatable);
    assert_eq!(
        own_property(font, "bold").map(|p| p.type_name),
        Some("QtQml.bool")
    );
    let text = own_type("QtQuick", "Text");
    assert_eq!(
        own_property(text, "font").map(|p| p.type_name),
        Some("QtQuick.font")
    );
    let qt = own_type("QtQml", "Qt");
    assert!(qt.is_singleton && !qt.is_creatable);
    let alignment = qt.enums.iter().find(|e| e.name == "Alignment").unwrap();
    assert!(alignment.is_flag);
    assert_eq!(alignment.alias, Some("AlignmentFlag"));
    assert!(alignment.keys.iter().any(|(key, _)| *key == "AlignLeft"));
    let has_key = |key: &str| {
        qt.enums
            .iter()
            .any(|e| e.keys.iter().any(|(known, _)| *known == key))
    };
    assert!(has_key("Key_Return") && has_key("LeftButton") && !has_key("NoSuchKey"));
}

/// The reader drops what it cannot place without a word, which is right for
/// a table of another Qt and wrong for one written here: every line of
/// Ljós's own table has to say something the table then has.
#[test]
fn every_line_of_the_own_table_is_understood() {
    let own = own();
    let is_enum = |name: &str| {
        let name = name.rsplit("::").next().unwrap_or(name);
        own.types.iter().any(|ty| {
            ty.enums
                .iter()
                .any(|e| e.name == name || e.alias == Some(name))
        })
    };
    let mut module = "";
    let mut class = "";
    for (number, line) in OWN.lines().enumerate() {
        let at = || format!("ljos.txt:{}: {line}", number + 1);
        let depth = line.len() - line.trim_start_matches('\t').len();
        let words: Vec<&str> = line.split_ascii_whitespace().collect();
        match (depth, words.as_slice()) {
            (0, ["module", uri]) => module = uri,
            (0, _) => assert!(line.is_empty() || line.starts_with('#'), "{}", at()),
            (1, ["import", uri, ..]) => assert!(own.modules.contains_key(uri), "{}", at()),
            (1, ["type", name, ..]) => class = name,
            (1, [name, class]) => {
                assert_eq!(
                    own.modules[module].exports.get(name),
                    own.classes.get(class),
                    "{}",
                    at()
                );
                assert!(own.classes.contains_key(class), "{}", at());
            }
            (2, ["prototype" | "attached", to, rest @ ..]) => {
                let found = if rest.contains(&"named") {
                    own.type_meant(to)
                } else {
                    own.classes.get(to).copied()
                };
                assert!(found.is_some(), "{}", at());
            }
            (2, ["prop", _, of, rest @ ..]) => {
                let found = if rest.contains(&"named") {
                    own.type_meant(of).is_some()
                } else {
                    own.classes.contains_key(of) || is_enum(of)
                };
                assert!(found, "{}", at());
            }
            (2, ["default", name]) => {
                let ty = &own.types[own.classes[class]];
                assert!(own_property(ty, name).is_some(), "{}", at());
            }
            (2, ["is" | "enum" | "flags" | "signal" | "method", ..]) => {}
            _ => panic!("{}", at()),
        }
    }
}

/// The two tables are chosen between by module, never by type.
const QT: &str = "\
module Shared
\tThing QThing
\ttype QThing Thing
\t\tprop theirs int
";

const BESIDE: &str = "\
module Shared
\tThing Shared.Thing
\tOther Shared.Other
\ttype Shared.Thing Thing
\t\tprop ours int
\ttype Shared.Other Other
module Mine
\tWidget Mine.Widget
\ttype Mine.Widget Widget
\t\tprototype Shared.Thing named
\t\tprop other Shared.Other named pointer
";

fn class_of(table: &Table, index: Option<usize>) -> Option<&'static str> {
    index.map(|index| table.types[index].class)
}

#[test]
fn a_module_both_tables_describe_is_read_from_qt_s() {
    let table = read(QT, BESIDE);
    let find =
        |uri: &str, name: &str| class_of(&table, table.type_named(uri, name, &mut Vec::new()));
    assert_eq!(find("Shared", "Thing"), Some("QThing"));
    // Not even the type only the own description has: the module is Qt's.
    assert_eq!(find("Shared", "Other"), None);
    assert!(!table.classes.contains_key("Shared.Thing"));
    assert!(!table.classes.contains_key("Shared.Other"));
    assert!(!table.modules["Shared"].is_own && table.modules["Mine"].is_own);
    let thing = &table.types[table.classes["QThing"]];
    assert!(!thing.is_own);
    assert_eq!(thing.properties.len(), 1);
    assert_eq!(thing.properties[0].name, "theirs");
    // The module only the own table has is read, and what it names in the
    // other is what Qt's table says that is.
    let widget = &table.types[table.classes["Mine.Widget"]];
    assert!(widget.is_own);
    assert_eq!(class_of(&table, widget.prototype), Some("QThing"));
    // A name Qt's module does not have stays as it was written.
    assert_eq!(widget.properties[0].type_name, "Shared.Other");
}

#[test]
fn without_qt_s_table_the_own_one_is_everything() {
    let table = read("", BESIDE);
    let find =
        |uri: &str, name: &str| class_of(&table, table.type_named(uri, name, &mut Vec::new()));
    assert_eq!(find("Shared", "Thing"), Some("Shared.Thing"));
    assert_eq!(find("Shared", "Other"), Some("Shared.Other"));
    assert!(table.modules.values().all(|module| module.is_own));
    let widget = &table.types[table.classes["Mine.Widget"]];
    assert_eq!(class_of(&table, widget.prototype), Some("Shared.Thing"));
    assert_eq!(widget.properties[0].type_name, "Shared.Other");
    // Neither table: nothing is known, and nothing panics.
    let empty = read("", "");
    assert!(empty.modules.is_empty() && empty.types.is_empty());
    assert_eq!(empty.type_named("Shared", "Thing", &mut Vec::new()), None);
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
\tThing Own.Thing
\tPart Own.Part
\ttype Own.Thing Thing
\t\tprototype Base.Item named
\t\tprop label Base.string named
\t\tprop template Base.Component named pointer
\t\tprop part Own.Part named pointer
\t\tprop theirs Base.Part named pointer
\t\tprop lost Base.Nothing named
\t\tprop plain QItem pointer
\ttype Own.Part Part
\t\tprototype Item
";

#[test]
fn a_named_type_is_what_a_file_that_imports_the_module_would_find() {
    let table = read(NAMED, "");
    let thing = &table.types[table.classes["Own.Thing"]];
    assert_eq!(class_of(&table, thing.prototype), Some("QItem"));
    let type_of = |name: &str| {
        let property = thing.properties.iter().find(|p| p.name == name).unwrap();
        (property.type_name, property.is_component)
    };
    // Through what the named module imports, as a file would find it.
    assert_eq!(type_of("label"), ("QString", false));
    assert_eq!(type_of("template"), ("QQmlComponent", true));
    // Two modules have a `Part`: the module named says which.
    assert_eq!(type_of("part"), ("Own.Part", false));
    assert_eq!(type_of("theirs"), ("QPart", false));
    // A name nothing has stays what was written.
    assert_eq!(type_of("lost"), ("Base.Nothing", false));
}

#[test]
fn a_type_that_is_not_named_is_a_class() {
    let table = read(NAMED, "");
    let thing = &table.types[table.classes["Own.Thing"]];
    let plain = thing.properties.iter().find(|p| p.name == "plain").unwrap();
    assert_eq!(plain.type_name, "QItem");
    // `Item` is a QML name: no class is called that.
    let part = &table.types[table.classes["Own.Part"]];
    assert_eq!(part.prototype, None);
}

#[test]
fn naming_a_type_of_another_module_gives_the_module_nothing() {
    let table = read(NAMED, "");
    let find =
        |uri: &str, name: &str| class_of(&table, table.type_named(uri, name, &mut Vec::new()));
    assert_eq!(find("Own", "Thing"), Some("Own.Thing"));
    assert_eq!(find("Own", "Item"), None);
    assert_eq!(find("Own", "string"), None);
    // An import does: Deep's `string` is Base's too.
    assert_eq!(find("Base", "string"), Some("QString"));
}

#[test]
fn the_own_table_says_the_class_of_a_part_a_control_holds() {
    // A member written through a part (`popup.scale`) is looked up on the
    // class the part's property says it holds: one that says only "an
    // object" has none of the part's members.
    let parts = [
        ("ComboBox", "popup", "QtQuick.Templates.Popup"),
        ("ComboBox", "indicator", "QtQuick.Item"),
        ("Control", "background", "QtQuick.Item"),
        ("Control", "contentItem", "QtQuick.Item"),
        ("AbstractButton", "indicator", "QtQuick.Item"),
        ("Popup", "background", "QtQuick.Item"),
        ("Popup", "contentItem", "QtQuick.Item"),
        ("Slider", "handle", "QtQuick.Item"),
        ("Dial", "handle", "QtQuick.Item"),
        ("Page", "header", "QtQuick.Item"),
        ("Page", "footer", "QtQuick.Item"),
        ("Dialog", "header", "QtQuick.Item"),
        ("Dialog", "footer", "QtQuick.Item"),
        ("ApplicationWindow", "background", "QtQuick.Item"),
        ("ApplicationWindow", "header", "QtQuick.Item"),
        ("ApplicationWindow", "footer", "QtQuick.Item"),
        ("ApplicationWindow", "menuBar", "QtQuick.Item"),
        ("GroupBox", "label", "QtQuick.Item"),
        ("MenuItem", "arrow", "QtQuick.Item"),
        ("TextField", "background", "QtQuick.Item"),
        ("TextArea", "background", "QtQuick.Item"),
        ("Label", "background", "QtQuick.Item"),
    ];
    for (name, part, class) in parts {
        let property = own_property(own_type("QtQuick.Templates", name), part)
            .unwrap_or_else(|| panic!("no {name}.{part}"));
        assert!(property.is_pointer, "{name}.{part}");
        assert_eq!(property.type_name, class, "{name}.{part}");
    }
}

#[test]
fn an_enums_name_is_an_enum_name() {
    // `Item.transformOrigin` is declared with the enum `TransformOrigin` in
    // either table; a property's type name that is no enum anywhere is not.
    assert!(is_enum_name("TransformOrigin"));
    assert!(!is_enum_name("NoSuchEnumOfAnyType"));
}

#[test]
fn a_module_ljos_builds_part_of_still_stands_in_for_the_rest() {
    // Of `QtQuick3D.Helpers` a type Ljós's own table has is itself
    // (`Type::is_built`); the module stands in for the others, as the
    // add-on modules beside it do.
    assert!(stands_in("QtQuick3D.Helpers"));
    assert!(stands_in("QtQuick3D.Effects"));
    assert!(stands_in("QtGraphs"));
    assert!(!stands_in("QtQuick3D"));
}
