---
title: Signals and Handlers
description: Declare custom signals, respond to events with signal and property change handlers, and listen to external targets with Connections.
section: language
order: 3
---

# Signals and Handlers

After reading this page, you will be able to declare and emit signals, write signal handlers and property change handlers, connect signals dynamically from JavaScript, and use `Connections` to listen to other objects.

## Declaring and emitting signals

You declare a signal on an object with the `signal` keyword, a name starting with a lower-case letter, and an optional parameter list in parentheses. Signal names cannot start with an upper-case letter or use reserved global names such as `print`, `gc`, or `Qt`.

```qml
import QtQuick

Item {
    id: root
    property int total: 0
    signal ping(int amount, string label)

    onPing: (amount, label) => {
        root.total += amount
    }

    Component.onCompleted: {
        ping(3, "start")
    }
}
```

To emit a signal, call it like a method (`ping(3, "start")` or `first.ping(3)`). When a signal declares typed parameters, Ljós converts the arguments passed by the caller to the declared parameter types before running any connected handlers.

## Signal handlers and parameter declaration

Every signal `<name>` has a corresponding handler attribute named `on<Name>`, with the first letter of the signal's name capitalised. You can write a handler as an arrow function or function expression that declares the parameters it uses:

```qml
import QtQuick

Item {
    id: root
    property string seen: ""

    Repeater {
        model: 2
        Item {
            objectName: "entry"
        }
        onItemAdded: (at, made) => {
            root.seen += made.objectName + ":" + at + " "
        }
    }
}
```

If you write a handler as a plain statement or block (`onFired: seen += why`) without declaring its parameters, the signal's named parameters are still injected into the handler's scope ahead of component `id`s and object properties. However, parameter injection is deprecated: the first time such a handler reads an undeclared parameter, Ljós logs a warning naming the parameter and its line and column (`Parameter 'why' is not declared. Injection of parameters into signal handlers is deprecated. Use JavaScript functions with formal parameters instead.`). A handler that declares its formal parameters—or does not read any of the signal's parameters—does not log a warning.

When a component exposes a child object through a property or an `alias`, you can also attach a signal handler through that grouped property, such as `form.button.onClicked: ...`.

## Property change handlers

Every property `<name>` automatically has a change signal `<name>Changed` and a corresponding handler `on<Name>Changed`. Attached properties also have change handlers, such as `LayoutMirroring.onEnabledChanged`.

```qml
import QtQuick

Item {
    id: root
    property int count: 0
    property int changes: 0

    onCountChanged: {
        changes += 1
    }

    function bump() {
        count = 5
        count = 5
    }
}
```

A property change handler runs only when the stored value actually changes:

- Assigning the value the property already holds breaks any binding on the property, but does not run `on<Name>Changed`.
- On a clamped property such as `Item.opacity` (which clamps to `0..1`), assigning `2` when `opacity` is already `1` does not fire `onOpacityChanged`.
- For `real` properties, `NaN` is not equal to itself (`NaN !== NaN`), so assigning `NaN` to a property that already holds `NaN` counts as a change and runs its change handler every time.
- On a `var` property holding an array or object, mutating a member in place (`box.n = 2` or `xs.push(1)`) does not fire the change handler, whereas mutating a `list<T>` property (`numbers.push(1)`) does.

When a property changes, the object's own `on<Name>Changed` handler runs before the bindings that depend on that property are re-evaluated, so inside the handler those downstream bindings still read their previous values. However, if a binding is read inside a signal that is emitted before the property change signal—such as reading a binding on `MouseArea.pressed` inside `onReleased`, which fires before `onPressedChanged`—Ljós evaluates the binding on read against the current state of `pressed` (`false`).

If a change handler writes a new value back to its own property (`onNChanged: if (n < 10) n = n + 1`), Ljós runs the handler again until it stops changing the property. If a handler never stops writing a new value, Ljós stops it after 1,000 runs in one update and logs a warning naming the property.

## Listening with `Connections` and `connect()`

To handle signals emitted by another object without writing the handler on that object itself, use a `Connections` item with `function on<Signal>(...)` methods:

```qml
import QtQuick

Item {
    id: root
    property int hits: 0
    property bool listening: true
    property Item source: first

    Item {
        id: first
        signal ping(int n)
    }

    Item {
        id: second
        signal ping(int n)
    }

    Connections {
        target: root.source
        enabled: root.listening
        function onPing(n) {
            root.hits += n
        }
    }
}
```

Changing `target` switches which object the `Connections` item listens to, and setting `enabled` to `false` pauses handling without disconnecting. Handlers on a `Connections` object (or connected imperatively) run after bindings that depend on the changed property have updated.

In JavaScript, reading a signal on an object returns a function object that emits the signal when called and provides `.connect(fn)` and `.disconnect(fn)` methods (inherited from `Function.prototype`) to attach or remove listener functions at run time.

## Timer and `Qt.callLater`

For time-based and deferred callbacks, QML provides `Timer` and `Qt.callLater`:

```qml
import QtQuick

Item {
    id: root
    property int ticks: 0
    property int batches: 0

    Timer {
        id: repeating
        interval: 100
        repeat: true
        running: true
        onTriggered: root.ticks += 1
    }

    function queueWork() {
        Qt.callLater(finishBatch)
        Qt.callLater(finishBatch)
    }

    function finishBatch() {
        batches += 1
    }
}
```

A `Timer` emits `triggered` once after `interval` milliseconds when `repeat` is `false`, or every `interval` milliseconds while `running` is `true` and `repeat` is `true`, and can be controlled with `start()` and `stop()`. `Qt.callLater(fn)` schedules `fn` to run after the current pass finishes; queuing the same function twice in a single pass runs it only once.

## See also

- [QML in Ljós](overview.md)
- [Properties and bindings](properties-and-bindings.md)
- [Components](components.md)
- [JavaScript in QML](javascript.md)
