---
title: Pointer Input and Gestures
description: Handle clicks, taps, drags, hover, wheel turns, pinches, and drag-and-drop with PointerArea, gestures, PinchArea, and DropArea.
section: data
order: 5
---
# Pointer Input and Gestures

You can handle pointer and touch input either with area items (`PointerArea`, `PinchArea`, and `DropArea`) or with lightweight gestures (`Tap`, `Drag`, `Pointer`, `Hover`, and `Wheel`) declared inside any visual item. Give interactive items an `Accessible.name` so screen readers can announce them and `ljos check` does not warn about unnamed interactive controls.

## PointerArea

The topmost enabled `PointerArea` under the pointer receives a click, with event coordinates (`mouseX`, or the event's position) reported in that `PointerArea`'s own coordinate system. If a `PointerArea` is disabled or sets `event.accepted = false` on a press, the press passes through to any `PointerArea` underneath it.

```qml
import Ljos.Ui

Item {
    id: root
    width: 200
    height: 200
    property int under: 0
    property int over: 0
    PointerArea {
        Accessible.role: Accessible.Button
        Accessible.name: "Under"
        width: 100
        height: 100
        onClicked: root.under++
    }
    PointerArea {
        Accessible.role: Accessible.Button
        Accessible.name: "Over"
        width: 50
        height: 50
        onClicked: root.over++
    }
}
```

The second `PointerArea` sits on top of the first, so clicking inside `(0..50, 0..50)` increments `over`, while clicking elsewhere in the 100×100 square increments `under`.

`PointerArea` also provides `onPressed`, `onReleased`, `pressedButtons`, `acceptedButtons`, `containsMouse`, and `hoverEnabled`:

- With `hoverEnabled: true`, moving the pointer into the area emits `entered` followed by a position change; moving within the area emits only position changes; moving out emits `exited` at the last position inside.
- Pressing a mouse button that nothing accepts does not end hover. Dragging outside while pressed emits `exited` before position changes continue, dragging back inside emits `entered`, and releasing outside leaves the area unhovered.

## Tap, Drag, and Pointer

A gesture recognises input on the item it is declared in, without a separate child item that owns geometry:

- `Tap` emits `tapped` and `singleTapped` on a tap, and `doubleTapped` when a second tap arrives shortly after the first. Its default `GesturePolicy` does not block a `PointerArea` beneath it.
- `Drag` watches a press without grabbing immediately: a move shorter than the system drag threshold does not start a drag, whereas a move past the threshold grabs the pointer and moves `target` (defaulting to its parent item). While dragging, `active` is true, `centroid` tracks the pointer, and `translation` reports the current drag delta before resetting to zero on release (while persistent translation accumulates across drags). If you disable an axis (`xAxis` or `yAxis`), drags along that axis will not start, and `target` stops at the axis's `maximum`.
- `Pointer` becomes `active` immediately on press, exposing `point` in its item's coordinates and `translation` as the pointer moves; releasing clears `point` while keeping `translation`, and does not block a tap on the same item.

```qml
import Ljos.Ui

Item {
    width: 300
    height: 200
    Rectangle {
        Accessible.role: Accessible.Button
        Accessible.name: "Box"
        id: box
        x: 50; y: 50
        width: 100; height: 100
        fill: "#cde"
        Tap {
            onTapped: box.fill = "#fc9"
            onDoubleTapped: box.fill = "#9cf"
        }
        Drag {
            target: box
            yAxis.maximum: 150
        }
    }
}
```

A tap turns `box` orange, a double tap turns it blue, and dragging moves `box` down up to `yAxis.maximum: 150`.

Note the subtle difference in drag threshold timing: `Drag` starts dragging on the first move that reaches the system drag distance, whereas `PointerArea` starts dragging on the move after passing the threshold.

## Hover and Wheel

A `Hover` tracks whether the pointer is over its parent item through `hovered`, `point`, and `active`. A `PointerArea` stacked above the item does not hide hover from a `Hover`.

A `Wheel` accumulates mouse wheel turns in `rotation` (where one standard wheel notch is 15 degrees), scales adjustments via `rotationScale` when a `target` property is configured, and stays `active` until `timeout` expires after the last wheel turn. Without a target property configured, a `Wheel` simply accumulates vertical (or horizontal) turns in `rotation`.

```qml
import Ljos.Ui

Item {
    width: 300
    height: 200
    Rectangle {
        id: panel
        x: 20; y: 20
        width: 120; height: 120
        fill: "#eee"
        Hover {
            id: hover
        }
        Wheel {
            id: wheel
            target: panel
        }
        Text {
            text: (hover.hovered ? "over" : "away") + " " + wheel.rotation
        }
    }
}
```

## DropArea and attached DragSource

To build internal drag-and-drop interactions, attach `DragSource` (`DragSource.active`, `DragSource.hotSpot.x`, `DragSource.hotSpot.y`, `DragSource.keys`, `DragSource.proposedAction`, `DragSource.dragType`, and `DragSource.drop()`) to the dragged item, and place a `DropArea` (`containsDrag`, `keys`, and the signals `entered`, `exited`, and `dropped`) where items can be dropped.

```qml
pragma Scripts: JavaScript
import Ljos.Ui

Item {
    width: 300
    height: 300
    DropArea {
        x: 150; y: 150
        width: 140; height: 140
        onEntered: (drag) => status.text = "entered"
        onExited: status.text = "left"
        onDropped: (drop) => status.text = "dropped " + drop.accepted
    }
    Rectangle {
        id: box
        width: 40; height: 40
        fill: "#fc9"
        DragSource.active: false
        DragSource.hotSpot.x: 20
        DragSource.hotSpot.y: 20
        PointerArea {
            Accessible.role: Accessible.Button
            Accessible.name: "Drag box"
            anchors.fill: parent
            onPressed: box.DragSource.active = true
            onReleased: box.DragSource.drop()
        }
    }
    Text {
        id: status
        y: 10
    }
}
```

- When `DragSource.active` becomes `true` while the item's `DragSource.hotSpot` is already inside a `DropArea`, `entered` fires immediately; if the drag starts outside and moves in later, `entered` fires when the hot spot crosses into the `DropArea`.
- Moving out of the `DropArea` or setting `DragSource.active = false` emits `exited`.
- Calling `DragSource.drop()` inside a `DropArea` emits `dropped` (the drag event carries `DragSource.proposedAction`) without emitting `exited`.

## PinchArea

A `PinchArea` scales its `pinch.target` between `pinch.minimumScale` and `pinch.maximumScale`, emitting `pinchStarted`, `pinchUpdated`, and `pinchFinished`. If both scale limits are set to `1`, the target keeps its scale unchanged.

```qml
import QtQuick

Item {
    width: 300
    height: 300
    Rectangle {
        id: box
        x: 100; y: 100
        width: 100; height: 100
        color: "#cde"
    }
    PinchArea {
        anchors.fill: parent
        pinch.target: box
        pinch.minimumScale: 0.5
        pinch.maximumScale: 2
    }
}
```

## Limits and edge cases

- On macOS and Windows, dragging out of the application window to external applications is not supported because `winit` 0.30.13 provides no API to initiate an OS drag session; drags stay within the window.
- `Wheel.margin` is stored as a property but has no effect on hit testing.

## See also

- [Keyboard Input, Shortcuts, and Focus](./keyboard-and-focus.md)
- [ListView and GridView](./list-and-grid-views.md)
- [PathView, TableView, TreeView, and Flickable](./other-views.md)
