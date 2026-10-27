---
title: Anchors and positioners
description: Place items relative to parents and siblings with anchors, or arrange children in sequences and grids with Row, Column, Flow and Grid.
section: ui
order: 2
---
# Anchors and positioners

Use `anchors` when you want an item's edges or centre to follow its parent or a sibling, and use the positioners `Row`, `Column`, `Flow` and `Grid` when you want to place a sequence of items side by side, in a stack, in wrapping lines or in a grid while keeping each child's own size.

## Anchors and margins

Anchors attach an item's edges or centres (`left`, `right`, `top`, `bottom`, `horizontalCenter`, `verticalCenter`, `baseline`, `fill`, `centerIn`) to its parent or a sibling. An anchored edge follows the edge it is anchored to, offset by its margin (`margins`, `leftMargin`, `rightMargin`, `topMargin`, `bottomMargin`, `horizontalCenterOffset`, `verticalCenterOffset`, `baselineOffset`).

When you combine `anchors.fill: parent` with `anchors.margins`, a side margin such as `anchors.leftMargin` overrides `anchors.margins` on that side. Any side margin you have never set follows `anchors.margins` when `anchors.margins` changes; once you set a specific side margin, it stops following `anchors.margins`.

Anchoring opposite edges resizes the item: an anchored `right` or `bottom` edge follows the parent when the parent resizes, and setting `left` together with `horizontalCenter` sets the item's width. Anchor targets can also be siblings, and an edge anchored to a sibling's edge follows that sibling when it moves.

`anchors.centerIn` places an item's centre on its target's centre and snaps the position to whole pixels by default. Set `anchors.alignWhenCentered: false` when you want to keep a fractional coordinate.

```qml
import QtQuick

Item {
    width: 200; height: 100

    Rectangle {
        anchors.fill: parent
        anchors.margins: 10
        anchors.leftMargin: 3
        color: "#eeeeee"
    }

    Rectangle {
        width: 51; height: 31
        anchors.centerIn: parent
        anchors.alignWhenCentered: false
        color: "#c03030"
    }

    Rectangle {
        width: 20; height: 20
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 4
        color: "#308030"
    }
}
```

This draws a pale panel inset by 10 pixels on every side except 3 on the left, a red box centred at a fractional x position (`74.5`), and a green square inset 4 pixels from the bottom-right corner.

Avoid these anchor pitfalls:
- You can only anchor to an item's parent or a sibling. Anchoring to any other item logs a warning and does nothing.
- Specifying `left`, `right` and `horizontalCenter` all at once logs a warning and keeps `left` and `right`.

## Row and Column

`Row` places its visible, non-empty children one after another along the x axis—from left to right by default, or from right to left when `layoutDirection` is `Qt.RightToLeft` (check `effectiveLayoutDirection` to read the resolved direction). `Column` places its visible, non-empty children down the y axis. Hidden children and children with zero size along the positioner's axis stay where they are.

Both `Row` and `Column` accept `spacing` between children and `padding` (`leftPadding`, `rightPadding`, `topPadding`, `bottomPadding`) around them, and size themselves to their positioned children, spacing and padding. When a child grows, the positioner lays out again.

```qml
import QtQuick

Item {
    width: 200; height: 120

    Row {
        spacing: 5; padding: 2
        Rectangle { width: 30; height: 20; color: "#2060a0" }
        Rectangle { width: 20; height: 30; color: "#c03030" }
        Rectangle { width: 40; height: 10; color: "#308030"; visible: false }
        Rectangle { width: 10; height: 10; color: "#c0a030" }
    }

    Column {
        y: 50
        spacing: 3; topPadding: 1
        Rectangle { width: 40; height: 10; color: "#2060a0" }
        Rectangle { width: 60; height: 20; color: "#c03030" }
    }
}
```

The row places its three visible squares from left to right with 5 pixels between them and 2 pixels of padding around them, skipping the hidden green rectangle. The column starts 1 pixel below its top and leaves 3 pixels between its two boxes.

Watch how the cross-axis coordinate and attached properties work:
- A `Row` leaves each child's `y` to the child, and a `Column` leaves each child's `x` to the child. The positioner offsets that cross-axis coordinate only by its padding on that side, and adjusts it by the difference when that padding changes.
- Positioned children expose `Positioner.index`, `Positioner.isFirstItem` and `Positioner.isLastItem`. The index counts only positioned children; a hidden child has no index.
- Every positioner (`Row`, `Column`, `Flow`, `Grid`) runs its `populate` transition when it first places its children and its `move` transition (alongside `add`) when a child changes place.

## Flow

`Flow` places its children side by side (`flow: Flow.LeftToRight` by default) and starts a new line when the next child does not fit the width. With `flow: Flow.TopToBottom`, it wraps at `height` instead and fills each column before starting the next.

```qml
import QtQuick

Flow {
    width: 100; spacing: 5
    Rectangle { width: 40; height: 10; color: "#2060a0" }
    Rectangle { width: 40; height: 20; color: "#2060a0" }
    Rectangle { width: 40; height: 15; color: "#2060a0" }
    Rectangle { width: 55; height: 5; color: "#2060a0" }
}
```

The first two boxes share the first line. The third does not fit beside them inside the 100-pixel width, so it starts a second line below the tallest box of the first line, and the fourth box fits beside it.

Keep in mind:
- A `Flow` with no `width` set stays on one line, and its implicit width is the sum of its visible children and spacing.
- With `layoutDirection: Qt.RightToLeft`, `Flow` starts each line at the right inside its padding, and re-wraps whenever its width changes.

## Grid

`Grid` arranges its children into `columns` and `rows` with `padding` (`leftPadding`, `rightPadding`, `topPadding`, `bottomPadding`) around the cells. If neither `columns` nor `rows` is set, `Grid` uses four columns; if only `rows` is set, the number of columns follows from the number of children. With `flow: Grid.TopToBottom` (instead of `Grid.LeftToRight`), it fills column by column.

`spacing` sets the gap between cells in both directions, while `rowSpacing` and `columnSpacing` override it for rows or columns. Within each cell, `horizontalItemAlignment` (`Grid.AlignLeft`, `Grid.AlignRight`, `Grid.AlignHCenter`) and `verticalItemAlignment` (`Grid.AlignTop`, `Grid.AlignBottom`, `Grid.AlignVCenter`) align each child. In a right-to-left `Grid`, columns count from the right and horizontal alignment flips (`effectiveHorizontalItemAlignment` reads `Grid.AlignRight`).

```qml
import QtQuick

Grid {
    columns: 2; spacing: 3; rowSpacing: 7
    horizontalItemAlignment: Grid.AlignHCenter
    verticalItemAlignment: Grid.AlignBottom
    Rectangle { width: 40; height: 10; color: "#2060a0" }
    Rectangle { width: 30; height: 20; color: "#2060a0" }
    Rectangle { width: 45; height: 25; color: "#2060a0" }
    Rectangle { width: 55; height: 31; color: "#2060a0" }
}
```

This creates two columns with 3 pixels between columns and 7 pixels between rows, centring each box horizontally in its cell and placing it on the cell's bottom edge.

Watch out for two common mistakes with `Grid` and positioners:
- If you set both `rows` and `columns` and give the `Grid` more children than `rows * columns`, any children beyond the last cell are left where they are.
- Never anchor a child along a positioner's axis (`top`, `bottom`, `verticalCenter`, `fill` or `centerIn` in a `Column`; `left`, `right`, `horizontalCenter`, `fill` or `centerIn` in a `Row`; or any anchor in a `Grid` or `Flow`). Doing so logs a warning (such as `QML Column: Cannot specify ... anchors for items inside Column. Column will not function.`), places nothing in that pass, and gives the positioner no implicit size. The positioner warns once on completion rather than on every pass. Only a child that the positioner skips (because it is invisible or has zero size along the axis) may have an anchor along that axis without warning.

## Differences from Qt

- Anchoring a child along a positioner's axis logs a warning, places nothing in that pass and leaves the positioner with no implicit size.
- Writing directly to a coordinate or size that an anchor or positioner has set is not yet implemented. <!-- check: scenarios in features/layouts/placed-geometry-writes.feature are marked @backlog -->
- `Positioner` is an uncreatable base type; instantiate `Row`, `Column`, `Grid` or `Flow` instead.
- Children beyond the last cell of a fixed `rows` by `columns` `Grid` are left where they are rather than wrapped into extra cells.

## See also

- [Visual items, rectangles and images](./items.md)
- [Layouts](./layouts.md)
