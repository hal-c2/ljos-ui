---
title: Modules and Imports
description: Import QML modules, directories, and JavaScript files, configure qmldir and singletons, and register native Rust types.
section: language
order: 5
---

# Modules and Imports

After reading this page, you will be able to import built-in modules, local directories, and JavaScript files, use namespace aliases, define singletons with `pragma Singleton` and `qmldir`, and understand how Ljós discovers modules and native Rust types.

## Importing modules, directories, and scripts

Every `.qml` file declares the modules, directories, and scripts it uses with `import` statements at the top of the file:

```qml
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Templates as T
import "parts"
import "rules.js" as Rules

Item {
    id: root
    width: 240
    height: 120
    property T.Control heldControl: actionButton

    Controls.Button {
        id: actionButton
        text: "Apply"
        focusPolicy: Controls.Button.ClickFocus
    }
}
```

Ljós supports three forms of `import`:

- **Module imports** (`import QtQuick`, `import QtQuick.Controls`, `import QtQuick.Layouts`, `import Qt.labs.synchronizer`): bring the types and singletons of a named module into scope. Adding `as <Namespace>` qualifies those types and their enumerations under `<Namespace>` (for example, `T.Control` or `T.ScrollBar.AlwaysOff`).
- **Directory imports** (`import "parts"` or `import "look"`): bring all `.qml` component files (and any `pragma Singleton` files or `qmldir` declarations) in that relative directory into scope.
- **JavaScript imports** (`import "rules.js" as Rules`): load a `.js` file and expose its top-level functions and variables under the qualifier `Rules`.

Enumeration keys can be read from any imported type, whether or not the script is inside an instance of that type (`Text.AlignHCenter`, `Font.Bold`, `ScrollBar.AlwaysOn`, `ScrollBar.Policy.AlwaysOff`), or through the module's import namespace (`T.ScrollBar.AlwaysOff`).

## Module discovery and `qmldir`

When building a project graph, Ljós discovers modules from `qmldir` files and from CMake `qt_add_qml_module` definitions. <!-- check: verify exact `qt_add_qml_module` arguments read by module discovery -->

A `qmldir` file in a module or directory lists the module's types and singletons:

```text
module MyProject.Style
singleton Theme 1.0 Theme.qml
Card 1.0 Card.qml
```

Note that `qmldir` directives `depends`, `import`, `optional import`, `prefer`, `plugin`, and `typeinfo` are not implemented yet.

When resolving a relative URL with `Qt.resolvedUrl(path)` or loading an asset from a component, Ljós resolves the address against the file where the call or component is written, following RFC 3986 (removing `.` and `..` segments and keeping a trailing slash on directory paths).

## Singletons

A QML singleton is a `.qml` file that begins with `pragma Singleton` and is registered with a `singleton` line in `qmldir` (or imported from a directory):

```qml
// Theme.qml
pragma Singleton
import QtQuick

QtObject {
    enum Mode { Light, Dark }
    property int base: 4
    property int twice: base * 2
    property color accent: "#336699"
    function scaled(n) { return n * base }
}
```

```qml
// Main.qml
import QtQuick

Item {
    width: 200
    height: 100
    property int marginSize: Theme.scaled(3)
    property int darkKey: Theme.Mode.Dark

    Rectangle {
        anchors.fill: parent
        anchors.margins: parent.marginSize
        color: Theme.accent
    }
}
```

A singleton is created lazily the first time a file reads it. By default, one instance is shared across the engine; if a QML singleton reads a per-window native singleton, Ljós creates one instance per window.

## Native Rust modules and `ljostypes.json`

Applications can expose native Rust types to QML using `#[derive(QmlObject)]`, `#[qml_impl]`, and `ljos::module!`. Properties declared on a Rust type are `final`, and `QtObject` is its implied base type (`objectName`).

During ahead-of-time compilation, `TypeOracle` resolves types from three sources:

1. Ljós's built-in type table.
2. The optional generated Qt type table (used by `ljos check --port`).
3. Your application's native Rust types, read either directly from `MetaObject`s in an `EngineOracle` or from a `ljostypes.json` file.

Running your application with `LJOS_TYPES=write` writes out `ljostypes.json`, which you can pass to `ljos::build::compile_dir_with_types` (or let `ljos::build::compile_dir` pick up) so that unknown properties or handlers on your native Rust types are caught as compile errors at build time.

## See also

- [QML in Ljós](overview.md)
- [Components and dynamic objects](components.md)
- [JavaScript in QML](javascript.md)
- [Porting from Qt](porting-from-qt.md)
