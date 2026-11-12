---
title: Layouts
description: Resize and align children across rows, columns, grids and pages using Row, Column, Grid, Pages and Layout attached properties.
section: ui
order: 3
---
# Layouts

Import `Ljos.Layout` and use `Row`, `Column`, `Grid` or `Pages` to place children next to each other, in cells, or one at a time. A child that says nothing through `Layout.` keeps its own size and is only placed; one that does is sized from its minimum, preferred and maximum constraints and can fill the room the container has.

## Row and Column

`Row` gives every visible child a cell in a row, one after the other with `spacing` between them (`Column` does the same down a column). On an axis a child sets nothing about through `Layout.`, the child keeps its own `width` (`height`): the row reads it and never writes it. On an axis it does, it asks for `Layout.preferredWidth` (or `Layout.preferredHeight`), else the `width` (`height`) its file gives it, else its `implicitWidth` (`implicitHeight`). The size the row gives a child is not taken as its request: hidden and shown again, the child asks for its own size.

When a `Row` has no explicit size of its own, it takes its implicit size from its children and `spacing`. An invisible (`visible: false`) child takes no cell and no spacing; when it becomes visible it gets a cell, and changing a child's preferred size lays the row out again.

When the layout is wider or narrower than the sum of its children's preferred sizes:
- **Growing**: children with `Layout.fillWidth: true` share the extra width in proportion to their preferred widths (or in the ratio of `Layout.horizontalStretchFactor` / `Layout.verticalStretchFactor` when set; a stretch factor of `0` leaves the child at its preferred size). A filling child with no preferred width takes whatever the other children leave. If a filling child hits its `Layout.maximumWidth`, it stops growing and the remaining filling children take the rest. When no child can grow, the children stay at the start of the row and the room is left at its end. Room that is shared out is shared in whole pixels, so the edges of filled children fall on pixels.
- **Shrinking**: when the row is narrower than its preferred sizes, `spacing` is subtracted before children shrink. Filling children shrink towards their `Layout.minimumWidth`, with larger children shrinking faster (stretch factors do not change how children shrink). A child at its minimum width keeps it while others shrink; below the sum of minimum widths, the children overflow the layout.

```qml
import Ljos.Ui
import Ljos.Layout

Row {
    width: 400; height: 20
    spacing: 4

    Rectangle {
        Layout.alignment: Align.VCenter
        implicitWidth: 100; implicitHeight: 20
        fill: "#2060a0"
        Layout.fillWidth: true
    }
    Rectangle {
        Layout.alignment: Align.VCenter
        implicitWidth: 50; implicitHeight: 20
        fill: "#c03030"
        Layout.fillWidth: true
        Layout.maximumWidth: 120
    }
    Rectangle {
        Layout.alignment: Align.VCenter
        Layout.preferredWidth: 30; Layout.minimumWidth: 20
        height: 20
        fill: "#308030"
    }
}
```

The blue and red boxes share the width left by the 30-pixel green box, with 4 pixels between cells. Because the red box is capped at `Layout.maximumWidth: 120`, it stops at 120 and the blue box takes the remaining width.

## Alignment, margins and layout direction

A child that is smaller than its cell is positioned inside the cell by `Layout.alignment` (`Align.Left`, `Align.Right`, `Align.HCenter`, `Align.Top`, `Align.Bottom`, `Align.VCenter`). By default a child sits at the start of its cell, at the left and the top; the first example centres its children with `Layout.alignment: Align.VCenter`. Across a `Row`, a child with `Layout.fillHeight: true` stretches up to its `Layout.maximumHeight`, while a child that does not fill keeps its preferred height.

`Column` without an explicit size is as wide as its widest child and as tall as all its visible children plus `spacing`. Both `Row` and `Column` also take `padding` (and `leftPadding`, `rightPadding`, `topPadding`, `bottomPadding`) between their edges and their children, and `layoutDirection`.

```qml
import Ljos.Ui
import Ljos.Layout

Column {
    spacing: 5
    width: 100; height: 300

    Rectangle { width: 50; height: 20; fill: "#2060a0" }
    Rectangle {
        width: 30; height: 40; fill: "#c03030"
        Layout.fillHeight: true
    }
    Rectangle {
        width: 30; height: 20; fill: "#308030"
        Layout.fillHeight: true
        Layout.fillWidth: true
    }
    Rectangle {
        width: 30; height: 20; fill: "#c0a030"
        Layout.alignment: Align.Right
    }
}
```

Here the red and green boxes share the remaining height of the 300-pixel column, the green box also fills the 100-pixel width, and the yellow box is aligned to the right edge of the column.

Margins (`Layout.margins`, `Layout.leftMargin`, `Layout.topMargin`, `Layout.rightMargin`, `Layout.bottomMargin`) sit inside the child's cell between the child and the cell's edge. Watch out when using `layoutDirection: Direction.RightToLeft`: the cells and `Layout.alignment` are mirrored, but `Layout.leftMargin` and `Layout.rightMargin` are not mirrored.

## Grid

`Grid` arranges visible children in a two-dimensional grid controlled by `columns`, `rows`, `rowSpacing`, `columnSpacing` (both `spacing` until set), `flow` (`Grid.LeftToRight` or `Grid.TopToBottom`) and `layoutDirection`, with the same size rule as `Row`.
- With `columns` set, it fills row by row; with `rows` set and `flow: Grid.TopToBottom`, it fills column by column. With only one count set, the other is as many as the children need. If neither `rows` nor `columns` is set, it forms a single line along its `flow`.
- Invisible children take no cell.
- `Layout.row` and `Layout.column` place a child in a specific cell, and the next child without explicit coordinates follows after it.
- `Layout.rowSpan` and `Layout.columnSpan` stretch a child across multiple cells. Spanning rows keeps the cells beside the child free for subsequent children; spanning columns widens the spanned columns in proportion to their preferred widths.
- Any column or row containing a filling child takes the extra space (up to `Layout.maximumWidth` / `Layout.maximumHeight`), while non-filling children are aligned within their column or row. Room no column or row takes stays at the end.

The example below is still written as `ljos migrate` leaves it, because the pass does not yet move a child placed with `Layout.row` and `Layout.column` into `Grid` (see [Bringing existing QML](../language/porting-from-qt.md)).

```qml
import QtQuick
import QtQuick.Layouts

GridLayout {
    columns: 3
    rowSpacing: 2; columnSpacing: 5

    Rectangle { width: 40; height: 10; color: "#2060a0"; Layout.row: 1; Layout.column: 1 }
    Rectangle { width: 20; height: 20; color: "#c03030" }
    Rectangle { width: 30; height: 12; color: "#308030" }
    Rectangle { width: 10; height: 8; color: "#c0a030"; Layout.column: 2 }
    Rectangle { width: 20; height: 30; color: "#7040a0"; Layout.rowSpan: 2 }
    Rectangle { width: 30; height: 6; color: "#208080" }
}
```

The blue box is placed at row 1, column 1; subsequent boxes fill the remaining cells in order, with the violet box spanning two rows.

## Pages and nested boxes

`Pages` shows the child at `currentIndex` and hides all the others, reporting the total number of children in its read-only `count` property. An empty `Pages` has `currentIndex: -1` and `count: 0`, and setting `currentIndex` to an index with no child hides every child.

`Pages` prefers the size of its largest child, and gives its own full size to the shown child unless that child sets `Layout.fillWidth: false` or `Layout.fillHeight: false` (in which case the child keeps its preferred size).

```qml
import Ljos.Ui
import Ljos.Layout

Item {
    width: 200; height: 100

    Pages {
        anchors.fill: parent
        currentIndex: tabs.currentIndex

        Rectangle { implicitWidth: 40; implicitHeight: 10; fill: "#2060a0" }
        Rectangle { implicitWidth: 60; implicitHeight: 30; fill: "#c03030" }
    }

    Row {
        id: tabs
        property int currentIndex: 0
        spacing: 4
        Rectangle {
            width: 40; height: 16; fill: "#444444"
            PointerArea {
                anchors.fill: parent
                Accessible.role: Accessible.Button
                Accessible.name: "First"
                onClicked: tabs.currentIndex = 0
            }
        }
        Rectangle {
            width: 40; height: 16; fill: "#444444"
            PointerArea {
                anchors.fill: parent
                Accessible.role: Accessible.Button
                Accessible.name: "Second"
                onClicked: tabs.currentIndex = 1
            }
        }
    }
}
```

At startup the blue panel fills the pages; clicking the second tab switches `currentIndex` to `1`, showing the red panel at the full size of the `Pages` and hiding the blue one.

You can also nest one `Row` or `Column` inside another, or place a `Loader` inside one (a `Loader` reports its loaded item's size as its implicit size). A nested box is a child like any other: it keeps the size its children give it, and takes the room the outer box leaves for it once it says `Layout.fillWidth: true` (or `Layout.fillHeight: true`).

## Limits

- Writing directly to a coordinate or size that an anchor or positioner has placed is not yet implemented. <!-- check: scenarios in features/layouts/placed-geometry-writes.feature are marked @backlog -->

## See also

- [Anchors and positioners](./positioning.md)
- [Visual items, rectangles and images](./items.md)
