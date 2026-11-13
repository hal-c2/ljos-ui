---
title: Controllers
description: Give an item behaviour by declaring a Controller in it, written as a QML file or as a TypeScript module, and act on its host when it is attached, detached and updated.
section: language
order: 9
---

# Controllers

After reading this page, you will be able to write a `Controller` as a QML file or as a TypeScript module, declare it inside the item it acts on, and use its lifecycle: `attached`, `detached` and `updated`.

## An item's behaviour, declared in it

Some behaviour belongs to an item without being drawn: it follows the pointer, orbits a camera, keeps a flag in step with the window's size. A `Controller`, from `Ljos.Core`, is a non-visual object that holds that behaviour. Declare one inside an item and that item is its `host`:

- `host` is the item it is declared in. A Controller declared inside another Controller shares that one's host. One declared in a popup acts on the popup's content item, where what is declared in a popup goes; if the popup's `contentItem` is replaced, it moves to the new one, with `detached()` and then `attached(host)`.
- `attached(host)` runs once the host is complete, or when the Controller is put into a host that already is.
- `detached()` runs when the host goes (a `Loader` unloading it, a delegate removed), when the Controller itself is destroyed, or when it leaves a popup's content item for the one that replaces it.
- `updated()` runs once after `attached`, then once after each turn in which a property of the host changed, after the bindings have settled and before the frame is drawn.

Everything else is what any object has: properties, functions, signals and children. A Controller's state is its properties, read and bound through its id, so there are no rules about the order of calls as React's hooks have.

## A Controller in a QML file

A file whose root is `Controller` is a type, named after the file. This one says whether its host is narrower than a width it is given:

```qml file=Breakpoint.qml group=breakpoint
// Breakpoint.qml
import Ljos.Core

Controller {
    property real width: 600
    readonly property bool compact: host !== undefined && host.width < width
}
```

```qml group=breakpoint
import Ljos.Ui

Rectangle {
    width: 480
    height: 240
    fill: size.compact ? "#f0e6d8" : "#d8e6f0"

    Breakpoint {
        id: size
        width: 720
    }
}
```

`compact` is a binding on the host's `width`, so it changes when the window does. Where a property follows from the host, a binding says it best. `updated` is for what a binding cannot say.

## Gestures in a Controller

The gestures of `Ljos.Ui` (`Tap`, `Drag`, `Pinch`, `Hover`, `Wheel` and `Pointer`) are Controllers too: their host is the item they act on. A gesture declared inside a Controller acts on the Controller's host, so a Controller can bundle the gestures of one behaviour. This one moves a viewpoint nearer or further with the wheel and back with a double tap. The view reads the clip planes from it, and they follow the distance through a conditional:

```qml file=Orbit.qml group=orbit
// Orbit.qml
import Ljos.Core
import Ljos.Ui

Controller {
    id: orbit
    readonly property real distance: Math.max(0.5, 50 - wheel.rotation / 3)
    // Near the target, the planes close in so depth keeps its precision.
    readonly property real clipNear: distance < 1 ? 0.01 : distance < 100 ? 0.1 : 1
    readonly property real clipFar: clipNear * 10000

    Wheel {
        id: wheel
    }
    Tap {
        onDoubleTapped: wheel.rotation = 0
    }
}
```

```qml group=orbit
import Ljos.Ui

Rectangle {
    width: 320
    height: 240
    fill: "#202830"

    Orbit {
        id: orbit
    }
    Text {
        color: "white"
        text: "at " + orbit.distance.toFixed(1) + ", clip " + orbit.clipNear + " to " + orbit.clipFar
    }
}
```

The Controller has no size to keep equal to its host's, as an invisible `Item` filling its parent would, and the clip planes are bindings the view reads, not values written from a handler that has to be called once by hand at the start.

## A Controller in a TypeScript module

A `.ts` module exports a Controller made by `controller`, which every module has without an import. It takes the Controller's typed `properties`, its `bindings` (each a function of the host and the Controller, evaluated again when what it reads changes, as a QML binding is) and its lifecycle as functions: `attached`, `detached` and `updated`. `attached` may return a function, which runs when the Controller is detached:

```ts file=breakpoint.ts group=module
// breakpoint.ts
export const Breakpoint = controller({
    properties: { width: 600 },
    bindings: {
        compact: (host, self) => host.width < self.width,
    },
    attached(host, self) {
        console.log("watching " + host.objectName + " from " + self.width)
        return () => console.log("stopped watching " + host.objectName)
    },
})
```

```qml group=module
import Ljos.Ui
import "breakpoint.ts" as Size

Rectangle {
    objectName: "page"
    width: 480
    height: 240
    fill: breakpoint.compact ? "#f0e6d8" : "#d8e6f0"

    Size.Breakpoint {
        id: breakpoint
        width: 720
    }
}
```

The module form has no `class` and no `this`: `self` is the Controller with its typed properties and bindings, and `host` is its host, an `Item`. A Controller from a module is named in QML through its import, `Size.Breakpoint`, and it is the same native object the QML form makes, so it is read and bound the same way. The checker types both: `width: "wide"` on a `Size.Breakpoint` is ``error: Invalid property assignment: number expected [invalid-assignment]``, and a key that is not one of the five is an error too. `properties` and `bindings` are written in the call, each member as `name: value`, since the Controller is made from the names written there: an object made elsewhere or a spread is an error.

## Where a Controller goes

A Controller acts on an item, so it is declared in one, or in a Controller that is. One held in a property of a plain object has no host and is never attached, and `ljos check` reports it as ``a `Controller` acts on the item it is declared in […] [controller-host]``.

## `updated` and idle

`updated` costs nothing unless a Controller handles it: the engine follows the host's properties only for Controllers with an `onUpdated` handler (or an `updated` function in a module). It runs once a turn however many properties changed, and what its handlers write does not run it again, so a Controller that handles it never keeps an idle app awake:

```qml
import Ljos.Core
import Ljos.Ui

Rectangle {
    id: root
    width: 320
    height: 240
    property int turns: 0

    Controller {
        onUpdated: root.turns = root.turns + 1
    }
    Text {
        text: "changed in " + root.turns + " turns"
    }
}
```
