---
title: QML in Ljós
description: How Ljós reads a QML file as a tree of objects, resolves names and types ahead of time, and compiles bindings into its intermediate representation.
section: language
order: 1
---

# QML in Ljós

After reading this page, you will understand how a `.qml` file forms a tree of objects in Ljós, how `id` and `parent` names are resolved, where semicolons belong, and what the compiler does with your files before your application runs.

## Objects and the tree

A QML file defines a single root object and the tree of child objects nested inside it. Every file begins with its `import` statements, followed by the root object declaration. Inside an object's braces, you write property assignments, property declarations, signals, JavaScript functions, and child objects.

```qml
import QtQuick

Item {
    id: root
    width: 200
    height: 100

    Rectangle {
        id: box
        width: 50
        height: 50
        color: "#336699"
    }

    Text {
        id: label
        x: box.width + 10
        text: "Ready"
        font.bold: true
        font.pixelSize: 16
    }
}
```

A child object written inside another object without a property name is added to the enclosing object's default property. For `Item` and types derived from it, that default property holds its visual children. Every QML object also inherits `QtObject` as its implied base and has an `objectName` string property.

An object's body is a set of declarations rather than a list of statements. A semicolon may end a value binding, a property declaration, or a signal declaration, where it is optional. Writing a semicolon after a child object, an object binding (`a: Item {}`), an object list (`data: [Item {}]`), a function, an enumeration, or a script block is a syntax error and stops the file from loading.

## Identifiers, `parent`, and name lookup

Any object in a file can be given an `id`, such as `id: root` or `id: box`. An `id` is an identifier within that component rather than a string property, and other bindings and handlers in the same component can read its properties and call its methods through that name.

When an expression reads an unqualified name, Ljós resolves it in a fixed order at compile time:

1. Local variables and parameters of the enclosing JavaScript function or signal handler.
2. Object `id`s declared in the component.
3. Properties, signals, and methods of the scope object where the expression is written.
4. Properties, signals, and methods of the component's root object.
5. Names from enclosing components and contexts, unless the file sets `pragma ComponentBehavior: Bound`.
6. Global objects and functions such as `Qt`, `Math`, `Date`, `console`, and `qsTr`.

A component written inside another, such as an inline delegate inside a `Repeater`, can read and assign to names belonging to the component around it. If the file starts with `pragma ComponentBehavior: Bound`, outer-context lookup is restricted so the component only sees names it declares or receives through required properties.

```qml
import QtQuick

Item {
    id: root
    property int created: 0
    property int total: 0

    Repeater {
        model: 3
        Item {
            Component.onCompleted: {
                created++
                total += index
            }
        }
    }
}
```

The name `parent` depends on the type of object it is read from. An `Item` has a visual `parent` property, and a `Timer` has a `parent` property that points to the object it is declared in. A plain `QtObject` has no `parent` property of its own, so reading `parent` inside a `QtObject` looks further out and resolves to `parent` on the root object of the file.

```qml
import QtQuick

Item {
    id: root
    property int step: 3
    property int ticks: 0

    Timer {
        id: timer
        property int twice: parent.step * 2
        interval: 50
        running: true
        onTriggered: parent.ticks += parent.step
    }
}
```

In a script expression, a QML type name is also a value that can appear on the right-hand side of `instanceof`. You can test `object instanceof Item`, `object instanceof Rectangle`, or `object instanceof MyComponent` for built-in types, file components, and inline components alike.Iterating over a QML object with `for (var n in obj)` lists the properties of its base types first, then the properties the type adds, followed by base signals and the type's own signals.

## Literal checking and unknown members

Values written directly in QML source are checked against their property's type when the file is read. Assigning `"tall"` to `height`, `1` to a `bool` property, `2.5` to an `int` property, or a primitive to a `list` property stops the file from loading and reports the line and column of the mistake.

```qml
import QtQuick

Item {
    width: 120
    height: 80
    visible: true
    objectName: "panel"
}
```

Property names, signal handlers, and grouped properties are also checked against the object's type. A misspelt property such as `widht: 100`, `border.widht: 2`, or `onWidhtChanged: {}` is rejected when the type is fully described in Ljós's type table or native Rust metadata. If a project contains a broken `.qml` file that no loaded component instantiates, the rest of the application still loads and runs.

## Ahead-of-time compilation and binding tiers

Ljós parses QML and embedded JavaScript ahead of time. When the parser encounters an error in one member of an object, it recovers at the next member and returns a partial syntax tree alongside its diagnostics, so `ljos check`, the language server, and hot reload can continue analysing the rest of the file.

From the syntax tree, the front end builds a project graph, resolves imports and types against its type oracle, and lowers each file into a serialisable intermediate representation (`ComponentDef` and `ObjectDef`). Each `ComponentDef` carries a structural signature hash of its declared properties, signals, and inline components so hot reload can tell whether a change is a body-only edit or a structural change. In production builds, `ljos::build::compile_dir` (or `ljos::build::compile_dir_with_types`) serialises this intermediate representation into the binary alongside the compiled JavaScript module.

Every binding and handler is classified at compile time into one of three execution tiers:

- **Tier 0**: The compiler resolves every identifier statically to its target (`Local`, `Id`, `ScopeProp`, `RootProp`, `OuterId`, `Attached`, `Enum`, `Singleton`, `ImportNs`, `Global`, `ContextProp`, or `Unknown`). There is no dynamic scope-chain walk at run time, and a direct property read is wired straight into the reactive graph.
- **Tier 1 (register VM)**: Bindings that use literals, resolved property reads and writes, value-type fields, arithmetic, comparisons, logical operators, template strings, pure built-ins (`Math.*`, `qsTr`, `Qt.rgba`, `Qt.point`, `Qt.size`, `Qt.rect`, `String.prototype.arg`), and calls to inlinable user functions are compiled to register bytecode. Tier 1 runs without starting the JavaScript engine and works on `var` properties as well as typed properties.
- **Tier 2 (QuickJS)**: Closures, loops, arrays, `JSON`, regular expressions, imported `.js` files, and imperative signal handlers compile to functions in the file's JavaScript module, taking their resolved references as parameters.

All three tiers read properties through the same reactive graph, so dependency tracking behaves identically regardless of which tier evaluates an expression. You can inspect tier coverage across a project with `ljos check --stats`. <!-- check: verify exact CLI output format of `ljos check --stats` -->

## See also

- [Properties and bindings](properties-and-bindings.md)
- [Signals and handlers](signals-and-handlers.md)
- [Components](components.md)
- [Modules and imports](modules-and-imports.md)
- [JavaScript in QML](javascript.md)
- [First app](../start/first-app.md)
