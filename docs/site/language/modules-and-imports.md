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

Ljós supports four forms of `import`:

- **Module imports** (`import Ljos.Ui`, `import Ljos.Controls`, `import Ljos.Design`, or the modules existing QML uses, such as `import QtQuick`, `import QtQuick.Controls` and `import QtQuick.Layouts`): bring the types and singletons of a named module into scope. Adding `as <Namespace>` qualifies those types and their enumerations under `<Namespace>` (for example, `T.Control` or `T.ScrollBar.AlwaysOff`).
- **Directory imports** (`import "parts"` or `import "look"`): bring all `.qml` component files (and any `pragma Singleton` files or `qmldir` declarations) in that relative directory into scope.
- **TypeScript imports** (`import "filter.ts" as Logic`): load a `.ts` module, check it, and expose what it exports under the qualifier `Logic`. Its types are checked against every call the QML makes.
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
pragma Scripts: JavaScript
import Ljos.Ui
import Ljos.Core

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
pragma Scripts: JavaScript
import Ljos.Ui

Item {
    width: 200
    height: 100
    property int marginSize: Theme.scaled(3)
    property int darkKey: Theme.Mode.Dark

    Rectangle {
        anchors.fill: parent
        anchors.margins: parent.marginSize
        fill: Theme.accent
    }
}
```

A singleton is created lazily the first time a file reads it. By default, one instance is shared across the engine; if a QML singleton reads a per-window native singleton, Ljós creates one instance per window.

## Native Rust modules and `ljostypes.json`

Applications can expose native Rust types to QML using `#[derive(QmlObject)]`, `#[qml_impl]`, and `ljos::module!`. Properties declared on a Rust type are `final`, and `QtObject` is its implied base type (`objectName`).

During ahead-of-time compilation, `TypeOracle` resolves types from three sources:

1. Ljós's built-in type table.
2. An optional table of the upstream modules' types, which `ljos check --port` compares against (see [Bringing existing QML](porting-from-qt.md)).
3. Your application's native Rust types, read either directly from `MetaObject`s in an `EngineOracle` or from a `ljostypes.json` file.

Running your application with `LJOS_TYPES=write` writes out `ljostypes.json`, which you can pass to `ljos::build::compile_dir_with_types` (or let `ljos::build::compile_dir` pick up) so that unknown properties or handlers on your native Rust types are caught as compile errors at build time. `ljos run` reads the same file to [preview a project without its Rust](../rust/native-types.md#previewing-without-your-rust).

## See also

- [QML in Ljós](overview.md)
- [Components and dynamic objects](components.md)
- [TypeScript and JavaScript](javascript.md)
- [Bringing existing QML](porting-from-qt.md)
