//! What a project's `qmldir` and `CMakeLists.txt` files say its modules are.

use qml_front::discover::{Module, Resource, Type, cmake, kept, qmldir};

fn module(uri: &str, types: &[(&str, &str)]) -> Module {
    Module {
        uri: uri.to_string(),
        types: types
            .iter()
            .map(|(name, path)| Type {
                name: (*name).to_string(),
                path: (*path).to_string(),
            })
            .collect(),
        resources: Vec::new(),
        prefix: "/qt/qml".to_string(),
    }
}

#[test]
fn a_qmldir_names_the_module_and_its_types() {
    let text = "module Thermostat\n\
        # what the designer sees\n\
        designersupported\n\
        plugin thermostatplugin\n\
        typeinfo plugins.qmltypes\n\
        depends QtQuick 2.0\n\
        singleton Constants 1.0 Constants.qml\n\
        internal Gauge Gauge.ui.qml\n\
        RoomView 1.0 views/RoomView.qml\n\
        Helpers 1.0 helpers.js\n";
    assert_eq!(
        qmldir(text),
        Some(module(
            "Thermostat",
            &[
                ("Constants", "Constants.qml"),
                ("Gauge", "Gauge.ui.qml"),
                ("RoomView", "views/RoomView.qml")
            ],
        ))
    );
    // A directory that is no module is only ever imported by its path.
    assert_eq!(qmldir("Clock 1.0 Clock.qml\n"), None);
}

#[test]
fn a_block_comment_declares_its_type_once() {
    // Qt reads the plain line; Ljós reads the block too, and either alone
    // declares the type.
    let text = "module App.Media\n\
        Player 1.0 Player.qml\n\
        # block Player 1.0 Player.qml Media \"Plays one source\"\n\
        #block Clock 1.0 Clock.qml\n\
        Clock 1.0 Clock.qml\n\
        # blocks Other 1.0 Other.qml\n\
        # block the layout below\n";
    assert_eq!(
        qmldir(text),
        Some(module(
            "App.Media",
            &[("Player", "Player.qml"), ("Clock", "Clock.qml")],
        ))
    );
}

#[test]
fn a_build_names_the_module_and_the_files_that_are_its_types() {
    let text = r#"
cmake_minimum_required(VERSION 3.16)
project(thermostat LANGUAGES CXX)

set(target_uri "Thermostat")
set(qml_singletons
    Constants.qml  # one object
)
list(APPEND qml_singletons "theme/Colors.qml")
set_source_files_properties(${qml_singletons} PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

if (ANDROID)
    message("QML_FILES ${qml_singletons}")
endif()

qt_add_qml_module(thermostat
    URI ${target_uri}
    VERSION 1.0
    NO_PLUGIN
    QML_FILES
        Main.qml
        main.qml
        views/RoomView.ui.qml
        "${CMAKE_CURRENT_SOURCE_DIR}/views/Dial.qml"
        ${qml_singletons}
        ${generated_elsewhere}
    RESOURCES
        images/Icon.qml.png
    SOURCES backend.cpp backend.h
)

qt6_add_qml_module(controls URI Thermostat.Controls QML_FILES Knob.qml)
"#;
    assert_eq!(
        cmake(text),
        [
            Module {
                resources: vec!["images/Icon.qml.png".to_string()],
                ..module(
                    "Thermostat",
                    &[
                        ("Main", "Main.qml"),
                        ("RoomView", "views/RoomView.ui.qml"),
                        ("Dial", "./views/Dial.qml"),
                        ("Constants", "Constants.qml"),
                        ("Colors", "theme/Colors.qml"),
                    ],
                )
            },
            module("Thermostat.Controls", &[("Knob", "Knob.qml")]),
        ]
    );
    // A module with no name is not one anything can import.
    assert_eq!(cmake("qt_add_qml_module(app QML_FILES Main.qml)"), []);
}

#[test]
fn a_build_names_the_files_the_program_keeps_with_a_module() {
    let text = r#"
qt_add_qml_module(Ast_Snow
    URI "Quick3DAssets.Snow"
    VERSION 1.0
    RESOURCE_PREFIX "/assets"
    QML_FILES Snow.qml
    RESOURCES
        snow.png
        "${CMAKE_CURRENT_SOURCE_DIR}/maps/sphere.png"
)
qt_add_qml_module(app URI Showroom RESOURCE_PREFIX / RESOURCES icons/close.svg)
"#;
    let modules = cmake(text);
    assert_eq!(modules[0].resources, ["snow.png", "./maps/sphere.png"]);
    assert_eq!(modules[0].prefix, "/assets");
    // The program names each by the module's name under where they are kept.
    assert_eq!(
        modules[0].address("snow.png"),
        "qrc:/assets/Quick3DAssets/Snow/snow.png"
    );
    assert_eq!(
        modules[0].address("./maps/sphere.png"),
        "qrc:/assets/Quick3DAssets/Snow/maps/sphere.png"
    );
    assert_eq!(
        modules[1].address("icons/close.svg"),
        "qrc:/Showroom/icons/close.svg"
    );
    // Under `qt/qml`, when the build does not say.
    let modules = cmake("qt_add_qml_module(app URI ColorPalette RESOURCES icons/qt.png)");
    assert_eq!(
        modules[0].address("icons/qt.png"),
        "qrc:/qt/qml/ColorPalette/icons/qt.png"
    );
}

#[test]
fn a_build_names_the_files_the_program_keeps_by_themselves() {
    let text = r#"
set(pictures images/one.png images/two.png)
qt_add_resources(app "data"
    PREFIX
        "/data"
    BASE
        "data"
    FILES
        "data/medals.csv"
)
qt6_add_resources(app "pictures" FILES ${pictures})
"#;
    let resource = |address: &str, path: &str| Resource {
        address: address.to_string(),
        path: path.to_string(),
    };
    assert_eq!(
        kept(text),
        [
            // By its path from `BASE`, under `PREFIX`.
            resource("qrc:/data/medals.csv", "data/medals.csv"),
            resource("qrc:/images/one.png", "images/one.png"),
            resource("qrc:/images/two.png", "images/two.png"),
        ]
    );
}
