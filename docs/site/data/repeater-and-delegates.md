---
title: Repeater, Instantiator, and DelegateChooser
description: Instantiate visual items or non-visual objects for every row in a model, and pick different delegates per row with DelegateChooser.
section: data
order: 3
---
# Repeater, Instantiator, and DelegateChooser

When you have a small collection of rows and want every item created upfront inside a positioner or layout, use `Repeater`. When you want to create non-visual `QtObject` instances from a model, use `Instantiator`. When different rows in a `Repeater` or view need different delegate components based on a role's value, wrap them in a `DelegateChooser`.

## Repeater and delegate lifecycle

A `Repeater` (`QtQuick`) creates one delegate item for every row of its `model`. Its delegates are inserted as siblings right before the `Repeater` in its parent, and the `Repeater` itself takes up no space.

`Repeater` exposes `model`, `delegate`, `count`, `itemAt(row)` (which returns the delegate for `row`, or nothing if `row` is outside the model), and the signals `itemAdded(index, item)` and `itemRemoved(index, item)`.

```qml
import QtQuick

Row {
    id: root
    property string picked: "?"
    function pick(i) {
        var it = rep.itemAt(i)
        picked = it ? it.objectName : "none"
    }
    Repeater {
        id: rep
        model: 3
        onItemAdded: (index, item) => root.picked = "added " + index
        Item {
            objectName: "d" + index
            width: 10
            height: 10
        }
    }
    Component.onCompleted: pick(2)
}
```

In this example, `onItemAdded` runs in order for each delegate after that delegate finishes `Component.onCompleted`, and `pick(2)` reads `rep.itemAt(2)` to find `"d2"`.

When the underlying model changes, `Repeater` updates only what is needed:

- Appending or inserting a row creates a single delegate at that position.
- Removing a row destroys only its delegate right after `itemRemoved` is emitted; removing several rows at once reports the first removed row index each time.
- Moving a row moves its existing delegate without creating a new one, and changing a role value updates bindings in place without recreating the delegate.
- Increasing a numeric model recreates the delegates; setting a model of `0` rows or calling `clear()` destroys every delegate; assigning a new model replaces every delegate.

Inside each delegate, you can access `index`, `model`, `modelData` (for models of plain values), and each role by name, or declare `required property` bindings such as `required property string name`, `required property int index`, or `required property var modelData`.

Keep these pitfalls in mind:

- A `Repeater` is not virtualised: it creates a delegate for every row at the start. Use `ListView` or `GridView` for larger models.
- If a delegate declares a `required property` whose name does not match any role, that delegate is not created.
- When a single row is removed from a model, its delegate's `index` becomes `-1` during removal. When an entire model is replaced, existing delegates keep their old `index` while being torn down.

## Choosing delegates with DelegateChooser

`DelegateChooser` and `DelegateChoice` (from `import QtQml.Models` or `import Qt.labs.qmlmodels`) let a `Repeater`, `ListView`, or other view pick a different delegate component for each row. Set `role` on the `DelegateChooser` to the name of a model role, and place `DelegateChoice` children inside it.

For each row, `DelegateChooser` picks the first `DelegateChoice` whose `roleValue` matches the row's value for `role` (comparing numbers by value). A `DelegateChoice` with no `roleValue` (and no other constraint such as `row` or `index`) acts as a fallback and matches any row.

```qml
import QtQuick
import QtQml.Models

Column {
    ListModel {
        id: lm
        ListElement { kind: "a" }
        ListElement { kind: "b" }
    }
    DelegateChooser {
        id: chooser
        role: "kind"
        DelegateChoice {
            roleValue: "a"
            Text { text: "A row " + index }
        }
        DelegateChoice {
            Text { text: "other " + kind }
        }
    }
    Repeater {
        model: lm
        delegate: chooser
    }
}
```

Here the first row matches `roleValue: "a"`, while the second row falls through to the choice without a `roleValue`.

Watch out for two behaviours with `DelegateChooser`:

- If no `DelegateChoice` matches a row, no delegate is created for that row, though the row is still counted in `count`.
- Changing the value of the role named by `chooser.role` destroys and recreates that row's delegate so a new choice can take effect, whereas changing any other role keeps the existing delegate instance.

## Creating non-visual objects with Instantiator

An `Instantiator` (`QtQml`, also reachable through `import QtQml.Models`) creates one `QtObject` (or any non-visual or visual object) per row of its `model`. It provides `model`, `delegate`, `count`, `active`, `object` (the first object created), `objectAt(row)`, and the `objectAdded` signal.

```qml
import QtQuick

Item {
    ListModel {
        id: names
        ListElement { label: "x" }
        ListElement { label: "y" }
    }
    Instantiator {
        id: inst
        model: names
        QtObject {
            property string shown: label
        }
    }
    Text {
        text: "count " + inst.count + " first " + (inst.object ? inst.object.shown : "none")
    }
}
```

Here `inst.count` is `2`, and `inst.object.shown` reads `"x"` from the first created `QtObject`.

- When you add a row to the model, `Instantiator` creates one object and emits `objectAdded`; when you remove a row, it destroys that row's object and shifts the remaining objects up.
- If you omit `model`, `Instantiator` creates a single object.
- Setting `active: false` destroys all created objects, and setting `active: true` creates them again. An asynchronous `Instantiator` populates its objects after a moment rather than immediately.

## Differences from Qt

- `Repeater` is not virtualised, matching Qt's semantics.
- A delegate's `index` becomes `-1` when its row is removed, matching Qt.
- `DelegateModel` with `parts` and `rootIndex` is not built.

## See also

- [Models in QML](./models.md)
- [ListView and GridView](./list-and-grid-views.md)
- [PathView, TableView, TreeView, and Flickable](./other-views.md)
