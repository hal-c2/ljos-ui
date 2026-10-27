---
title: Properties and Bindings
description: Declare typed, readonly, default, and alias properties in QML, write reactive bindings, and control how values update and convert.
section: language
order: 2
---

# Properties and Bindings

After reading this page, you will be able to declare properties of any QML type, write reactive bindings and aliases, replace or restore bindings from JavaScript, and predict how values convert and update.

## Property declarations

You declare a custom property on an object with `property <type> <name>`, optionally followed by a colon and an initial value or binding. A property name must start with a lower-case letter and cannot reuse a reserved global name such as `print`, `console`, `Math`, `Qt`, `NaN`, `undefined`, `eval`, `isNaN`, or `qsTr`.

```qml
import QtQuick
import QtQuick.Templates as T

Item {
    id: root
    property int count: 0
    property real ratio: 0.5
    property bool active: true
    property string label: "Ready"
    property url homepage: "https://example.org/"
    property var payload: ({ items: [] })
    property list<int> numbers: [1, 2]
    property T.Control currentControl
    readonly property int doubled: count * 2
}
```

A property marked `readonly` must be initialised where it is declared and cannot be assigned to later, although `Component.createObject` can pass an initial override for it. A property declared with an object type (including one qualified through an import namespace such as `T.Control`) starts as `null` until an object is assigned to it. Properties declared on native Rust types with `#[derive(QmlObject)]` are `final` and cannot be shadowed in QML.

A component can also mark one property as `default property`. Children written inside the component's own definition still go into the default property of the base type it extends; only children written by users of the component are directed into the component's declared `default property`.

## Value types, colours, enums, and lists

QML includes structured value types (`point`, `size`, `rect`, `vector2d`, `vector3d`, `vector4d`, `quaternion`, `color`, and `font`). Two value-type instances with the same contents compare equal with both `==` and `===`. Most value-type properties start at zero when declared without a value, except `size`, which starts at `-1` by `-1`, and `color`, which starts as an invalid colour (`unset.valid === false`, while reading as `"#000000"`).

You can bind or assign individual fields of a value-type property, grouped property, or attached property using either dotted notation or a group block:

```qml
import QtQuick

Item {
    id: root
    property real scaleFactor: 3
    property point spot
    spot.x: 10
    spot.y: 4

    property color tint
    tint.g: root.scaleFactor / 6
    tint.a: 0.25

    Text {
        id: heading
        text: "Title"
        font {
            bold: true
            pixelSize: 19
        }
    }

    Rectangle {
        width: 80
        height: 40
        border.width: 3
        border.color: Qt.lighter("#336699", 1.2)
    }
}
```

Colour helpers on `Qt` include `Qt.rgba`, `Qt.hsla`, `Qt.hsva`, `Qt.color`, `Qt.lighter`, `Qt.darker`, `Qt.tint`, `Qt.alpha`, and `Qt.colorEqual`. A font's `pixelSize` must be a positive integer (fractions are rounded, zero or negative values are ignored, and `NaN` throws an error from script), and its `weight` is held between `1` and `1000`.

Enumeration properties such as `Text.horizontalAlignment` or `ScrollBar.policy` accept enum keys (`Text.AlignHCenter`, `ScrollBar.AlwaysOn`, `ScrollBar.Policy.AlwaysOff`) or key names as strings (`"AlignHCenter"`). Plain numbers written directly in QML source for an enum property are rejected when the file loads, whereas numbers assigned from JavaScript are rounded (or throw an `Error` if `NaN`). Flag properties combine keys with bitwise OR, such as `Qt.LeftButton | Qt.RightButton` or `Popup.CloseOnEscape | Popup.CloseOnPressOutside`.

For sequences, `var` and `list<T>` behave differently:

- A `var` property holding a JavaScript array or object stores the script's own reference. Calling `xs.push(4)` or mutating `box.n = 2` mutates it in place without emitting a property change signal.
- A `list<T>` property (`list<int>`, `list<string>`, `list<var>`, `list<QtObject>`) owns its sequence. Assigning or binding an array to a `list<T>` copies the array and any nested arrays or objects inside it. Calling `numbers.push(3)` or assigning `numbers[0] = 9` mutates the property itself, emits its change signal, and leaves any binding on the list property in place. On a `readonly property list<string>`, calling `push` throws a `TypeError`.

## Reactive bindings and `Qt.binding`

Any JavaScript expression on the right-hand side of `:` in QML sets up a reactive binding. Whenever a property read by the binding changes, the bound property updates automatically.

You can also write a block binding in braces `{ ... }`. In QML, a block binding evaluates to its last expression statement without needing an explicit `return` keyword (unless the braces start with a string or number followed by a colon, such as `{ "k": 1 }`, which is parsed as an object literal).

An imperative assignment (`b = 10`) in a function or handler breaks an existing binding on `b`, even if the assigned value equals the value `b` already held. To install a new reactive binding from JavaScript, assign `Qt.binding(fn)`:

```qml
import QtQuick

Item {
    id: root
    property int a: 1
    property int b: a * 2

    function freeze() {
        b = b
    }

    function follow() {
        b = Qt.binding(function() { return a * 3 })
    }
}
```

While a component is being constructed, a binding might read a property that is only initialised later in `Component.onCompleted`. Ljós holds any error thrown during creation until construction finishes; if `Component.onCompleted` sets the missing dependency so the binding succeeds, no warning is logged.

If two bindings read each other and do not settle to the same value, Ljós stops the cycle and logs a `Binding loop detected for property ...` warning at the line and column of the property name. If a cyclic dependency settles to the value the property already holds (for example through `Math.max` and `Math.min`), no loop warning is reported.

## Property aliases

An `alias` property forwards reads, writes, and bindings to another object, a property of an object, a grouped property, or a single member of a value-type property:

```qml
import QtQuick

Item {
    id: root
    property color tint: Qt.rgba(0, 0.75, 0, 0)
    property alias green: root.tint.g
    property alias frame: box.border
    property alias px: label.font.pixelSize
    default property alias content: body.data

    Rectangle {
        id: box
        width: 100
        height: 60
        border.color: "green"
        border.width: 1
        Item {
            id: body
            anchors.fill: parent
            anchors.margins: 8
        }
    }

    Text {
        id: label
        text: "Sample"
        font.pixelSize: 12
        font.italic: true
    }
}
```

When an alias points to an object or a group (`property alias frame: box.border` or `property alias counter: inner`), users of the component can bind sub-properties and signal handlers through it (`frame.width: 3`, `counter.onClicked: ...`) without disturbing the other members of the group.

Wait: if a `url` property is set through an alias from a file in another directory, the relative URL string is kept as written, and the receiving component (such as an `Image` inside the component) resolves it relative to its own file.

## Resetting properties and bidirectional synchronisation

Some built-in properties—such as `Item.width`, `Item.height`, `anchors.*`, `Text.baseUrl`, `Control.baselineOffset`, `Control.padding` (and side/axis paddings), `Pane.contentWidth`, `Pane.contentHeight`, `Loader.sourceComponent`, and `LayoutMirroring.enabled`—have a default state they return to when assigned `undefined`. Properties without a reset state (and native Rust properties without `#[qml(reset)]`) refuse `undefined` and keep their current value.

```qml
import QtQuick

Rectangle {
    implicitWidth: 40
    implicitHeight: 30
    width: 80
    height: 60

    function loosen() {
        width = undefined
        height = undefined
    }
}
```

To keep two or more properties in step without breaking their bindings, import `Qt.labs.synchronizer` and declare a `Synchronizer`:

```qml
import QtQuick
import Qt.labs.synchronizer

Item {
    id: root
    property int base: 3
    property int a: base * 2
    property int b: 1

    Synchronizer {
        sourceObject: root
        sourceProperty: "a"
        targetObject: root
        targetProperty: "b"
    }
}
```

When a `Synchronizer` is created, the source gives its initial value to the targets, and subsequent changes to either side update the other while preserving existing bindings.

## See also

- [QML in Ljós](overview.md)
- [Signals and handlers](signals-and-handlers.md)
- [JavaScript in QML](javascript.md)
- [States and transitions](states-and-transitions.md)
