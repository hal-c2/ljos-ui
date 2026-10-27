---
title: Layouts
description: Resize and align children across rows, columns, grids and stacks using RowLayout, ColumnLayout, GridLayout, StackLayout and Layout attached properties.
section: ui
order: 3
---
# Layouts

Import `QtQuick.Layouts` and use `RowLayout`, `ColumnLayout`, `GridLayout` or `StackLayout` when you want the container to calculate and assign its children's sizes from their minimum, preferred and maximum constraints rather than merely positioning them at their existing sizes.

## RowLayout and ColumnLayout

`RowLayout` gives every visible child a cell in a row and distributes its own width among them (`ColumnLayout` does the same for height down a column). Each child's preferred size comes from `Layout.preferredWidth` (or `Layout.preferredHeight`), falling back to `implicitWidth` (`implicitHeight`) and then `width` (`height`). Once a layout has taken a child's `width`, a later write to that `width` does not change the row's implicit width.

When a `RowLayout` has no explicit size of its own, it takes its implicit size from its children and `spacing`. An invisible (`visible: false`) child takes no cell and no spacing; when it becomes visible it gets a cell, and changing a child's preferred size lays the row out again.

When the layout is wider or narrower than the sum of its children's preferred sizes:
- **Growing**: children with `Layout.fillWidth: true` share the extra width in proportion to their preferred widths (or in the ratio of `Layout.horizontalStretchFactor` / `Layout.verticalStretchFactor` when set; a stretch factor of `0` leaves the child at its preferred size). A filling child with no preferred width takes whatever the other children leave. If a filling child hits its `Layout.maximumWidth`, it stops growing and the remaining filling children take the rest. When no child can grow, the cells grow while the children keep their sizes.
- **Shrinking**: when the row is narrower than its preferred sizes, `spacing` is subtracted before children shrink. Filling children shrink towards their `Layout.minimumWidth`, with larger children shrinking faster (stretch factors do not change how children shrink). A child at its minimum width keeps it while others shrink; below the sum of minimum widths, the children overflow the layout.

```qml
import QtQuick
import QtQuick.Layouts

RowLayout {
    width: 400; height: 20
    spacing: 4

    Rectangle {
        implicitWidth: 100; implicitHeight: 20
        color: "#2060a0"
        Layout.fillWidth: true
    }
    Rectangle {
        implicitWidth: 50; implicitHeight: 20
        color: "#c03030"
        Layout.fillWidth: true
        Layout.maximumWidth: 120
    }
    Rectangle {
        Layout.preferredWidth: 30; Layout.minimumWidth: 20
        height: 20
        color: "#308030"
    }
}
```

The blue and red boxes share the width left by the 30-pixel green box, with 4 pixels between cells. Because the red box is capped at `Layout.maximumWidth: 120`, it stops at 120 and the blue box takes the remaining width.

## Alignment, margins and layout direction

A child that is smaller than its cell is positioned inside the cell by `Layout.alignment` (`Qt.AlignLeft`, `Qt.AlignRight`, `Qt.AlignHCenter`, `Qt.AlignTop`, `Qt.AlignBottom`, `Qt.AlignVCenter`). By default a child sits at the left and is vertically centred. Across a `RowLayout`, a child with `Layout.fillHeight: true` stretches up to its `Layout.maximumHeight`, while a child that does not fill keeps its preferred height.

`ColumnLayout` without an explicit size is as wide as its widest child and as tall as all its visible children plus `spacing`. Both `RowLayout` and `ColumnLayout` also support `uniformCellSizes` and `layoutDirection`.

```qml
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    width: 100; height: 300

    Rectangle { width: 50; height: 20; color: "#2060a0" }
    Rectangle {
        width: 30; height: 40; color: "#c03030"
        Layout.fillHeight: true
    }
    Rectangle {
        width: 30; height: 20; color: "#308030"
        Layout.fillHeight: true
        Layout.fillWidth: true
    }
    Rectangle {
        width: 30; height: 20; color: "#c0a030"
        Layout.alignment: Qt.AlignRight
    }
}
```

Here the red and green boxes share the remaining height of the 300-pixel column, the green box also fills the 100-pixel width, and the yellow box is aligned to the right edge of the column.

Margins (`Layout.margins`, `Layout.leftMargin`, `Layout.topMargin`, `Layout.rightMargin`, `Layout.bottomMargin`) sit inside the child's cell between the child and the cell's edge. Watch out when using `layoutDirection: Qt.RightToLeft`: the cells and `Layout.alignment` are mirrored, but `Layout.leftMargin` and `Layout.rightMargin` are not mirrored.

## GridLayout

`GridLayout` arranges visible children in a two-dimensional grid controlled by `columns`, `rows`, `rowSpacing`, `columnSpacing`, `flow` (`GridLayout.LeftToRight` or `GridLayout.TopToBottom`), `layoutDirection`, `uniformCellWidths` and `uniformCellHeights`.
- With `columns` set, it fills row by row; with `rows` set and `flow: GridLayout.TopToBottom`, it fills column by column. If neither `rows` nor `columns` is set, it forms a single line along its `flow`.
- Invisible children take no cell.
- `Layout.row` and `Layout.column` place a child in a specific cell, and the next child without explicit coordinates follows after it.
- `Layout.rowSpan` and `Layout.columnSpan` stretch a child across multiple cells. Spanning rows keeps the cells beside the child free for subsequent children; spanning columns widens the spanned columns in proportion to their preferred widths.
- Any column or row containing a filling child takes the extra space (up to `Layout.maximumWidth` / `Layout.maximumHeight`), while non-filling children are aligned within their column or row. Setting `uniformCellWidths` or `uniformCellHeights` makes every column or row as large as the largest child requires.

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

## StackLayout and nested layouts

`StackLayout` shows the child at `currentIndex` and hides all the others, reporting the total number of children in its read-only `count` property. An empty `StackLayout` has `currentIndex: -1` and `count: 0`, and setting `currentIndex` to an index with no child hides every child.

`StackLayout` prefers the size of its largest child, and gives its own full size to the shown child unless that child sets `Layout.fillWidth: false` or `Layout.fillHeight: false` (in which case the child keeps its preferred size).

```qml
import QtQuick
import QtQuick.Layouts

Item {
    width: 200; height: 100

    StackLayout {
        anchors.fill: parent
        currentIndex: tabs.currentIndex

        Rectangle { implicitWidth: 40; implicitHeight: 10; color: "#2060a0" }
        Rectangle { implicitWidth: 60; implicitHeight: 30; color: "#c03030" }
    }

    Row {
        id: tabs
        property int currentIndex: 0
        spacing: 4
        Rectangle {
            width: 40; height: 16; color: "#444444"
            MouseArea {
                anchors.fill: parent
                Accessible.role: Accessible.Button
                Accessible.name: "First"
                onClicked: tabs.currentIndex = 0
            }
        }
        Rectangle {
            width: 40; height: 16; color: "#444444"
            MouseArea {
                anchors.fill: parent
                Accessible.role: Accessible.Button
                Accessible.name: "Second"
                onClicked: tabs.currentIndex = 1
            }
        }
    }
}
```

At startup the blue panel fills the stack; clicking the second tab switches `currentIndex` to `1`, showing the red panel at the full stack size and hiding the blue one.

You can also nest one layout inside another, or place a `Loader` inside a layout (a `Loader` reports its loaded item's size as its implicit size). An inner layout takes the room the outer layout leaves for it, and its minimum width is the sum of its children's minimum widths. If you tell a nested layout not to fill, it keeps its preferred size and is aligned like any other child; a nested layout that sets an explicit preferred size keeps it whether or not its tallest child is visible.

## Differences from Qt

- Writing directly to a coordinate or size that an anchor or positioner has placed is not yet implemented. <!-- check: scenarios in features/layouts/placed-geometry-writes.feature are marked @backlog -->

## See also

- [Anchors and positioners](./positioning.md)
- [Visual items, rectangles and images](./items.md)
