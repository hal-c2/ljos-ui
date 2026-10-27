---
title: PathView, TableView, TreeView, and Flickable
description: Arrange items along a curve with PathView, display two-dimensional tables and trees with TableView and TreeView, or build custom scrollable areas with Flickable.
section: data
order: 4
---
# PathView, TableView, TreeView, and Flickable

Beyond linear lists and uniform grids, `QtQuick` provides specialised views and scrolling containers: `PathView` distributes delegates along a geometric path, `TableView` lays out two-dimensional rows and columns, `TreeView` displays expandable hierarchical rows, and `Flickable` scrolls any child content larger than its viewport.

## PathView

A `PathView` spaces its delegates evenly along a `Path` built from segments (`PathLine`, `PathQuad`, `PathCubic`, `PathArc`, and `PathAttribute`). Set `pathItemCount` to limit how many rows sit on the path at once; a current row outside that count waits at the end of the path, and increasing `pathItemCount` brings other rows onto the path.

You can shift every row along the path with `offset`. Each delegate can inspect `PathView.view`, `PathView.isCurrentItem`, and `PathView.onPath` (a read-only boolean that is true while the delegate is on the path).

```qml
import QtQuick

PathView {
    width: 500
    height: 300
    model: 4
    pathItemCount: 3
    delegate: Rectangle {
        width: 20
        height: 10
        color: PathView.isCurrentItem ? "#ffcc00" : "#dddddd"
    }
    path: Path {
        startX: 0; startY: 50
        PathLine { x: 400; y: 50 }
    }
}
```

Three of the four rows sit on the straight line at once, and the current row takes the highlight colour.

- Stepping `currentIndex` wraps around the ends of the model, and moving to a new current row animates it to the start of the path over `highlightMoveDuration` (or jumps at once when `highlightMoveDuration` is `0`).
- Places along the path are counted from `preferredHighlightBegin` when a highlight range is active; with no highlight range (`highlightRangeMode`), `offset` and `currentIndex` move independently.
- After a flick, items coast and decelerate smoothly. With `snapMode` set to `PathView.SnapToItem`, a flick settles on the nearest item; with `PathView.SnapOneItem`, a flick advances at most one item.

## TableView and TableModel

A `TableView` (`QtQuick`) displays a grid of cells from a `TableModel` (`import Qt.labs.qmlmodels`, with one `TableModelColumn` per column and data in `rows`) or from a single-column model such as an array, a number, or a `ListModel`. In a `TableModel`, each delegate reads its cell value via `display` or through `required property` bindings; if the model is an array of objects, each delegate receives its object. Appending rows to the model updates the table, and a delegate can write back to its role in the model.

```qml
import QtQuick
import Qt.labs.qmlmodels

TableView {
    width: 300
    height: 200
    model: TableModel {
        TableModelColumn { display: "name" }
        TableModelColumn { display: "size" }
        rows: [ { name: "a.txt", size: 1 }, { name: "b.txt", size: 2 } ]
    }
    delegate: Text {
        text: display
        implicitWidth: 100
        implicitHeight: 20
    }
}
```

You can customise sizing, selection, editing, and synchronisation with `columnWidthProvider`, `rowHeightProvider`, `selectionModel`, `editTriggers`, `editDelegate`, `syncView`, `syncDirection`, `reuseItems`, `resizableColumns`, `resizableRows`, `moveColumn()`, `moveRow()`, `cellAtIndex()`, and `itemAtCell()`.

`TableView` is a `FocusScope`: arrow keys move the current cell, and `Tab` past the last cell wraps to the first cell (or moves focus out of the table when key navigation is disabled).

Watch out for sizing defaults: if a cell delegate has no `implicitWidth` or `implicitHeight`, `TableView` treats that dimension as `50` pixels, and if the `TableView` itself has no room for a cell, no cells are laid out.

## TreeView and TreeModel

A `TreeView` (`QtQuick`) displays hierarchical data, such as a `TreeModel` (`import Qt.labs.qmlmodels`) whose `rows` contain nested `rows`. At first, `TreeView` shows only its top-level rows. Every cell in the table has a delegate, and the first column acts as the tree node.

Each delegate can declare `required property` bindings for `depth`, `expanded`, `hasChildren`, `isTreeNode`, `treeView`, and the column role (such as `display`). You can expand and collapse branches from script with `toggleExpanded()`, `expandRecursively()`, `collapseRecursively()`, and `expandToIndex()`, and navigate cells with the arrow keys.

```qml
import QtQuick
import Qt.labs.qmlmodels

TreeView {
    width: 200
    height: 300
    model: TreeModel {
        TableModelColumn { display: "name" }
        rows: [
            { name: "docs", rows: [ { name: "a" } ] },
            { name: "bin" }
        ]
    }
    delegate: Item {
        implicitWidth: 120
        implicitHeight: 20
        required property bool hasChildren
        required property bool expanded
        required property int depth
        required property string display
        Text { x: depth * 10; text: display }
    }
}
```

- Expanding a row reveals its child rows, collapsing hides them, and adding a child row under an already-expanded row displays it immediately. Expanding a row that has no children leaves it expanded with no child rows added.
- When backed by a lazy folder model, a folder that has not been read yet still shows its expand arrow; expanding it reads the folder once and fills in its children when ready.

## Flickable

A `Flickable` scrolls child items across a viewport. Any visual item you declare inside a `Flickable` is parented to `contentItem`, which has size `contentWidth` by `contentHeight` (defaulting to the `Flickable`'s own width and height if unset).

```qml
import QtQuick

Flickable {
    width: 200
    height: 200
    contentWidth: 200
    contentHeight: 600
    Rectangle { width: 200; height: 600; color: "#cde" }
}
```

You can inspect and control scrolling through `contentX`, `contentY`, `originX`, `originY`, `flickableDirection`, `boundsBehavior`, `returnToBounds()`, `resizeContent()` (which resizes the content while keeping a chosen point stationary), `topMargin`, `leftMargin`, `bottomMargin`, `rightMargin`, and `visibleArea`.

- The margins count as part of the scrollable range: content starts after `topMargin` and `leftMargin`, and the scrollable end extends past the content by `bottomMargin` and `rightMargin`.
- When the content is smaller than the `Flickable`, the whole content is visible, and `visibleArea` reports the visible fraction and start offset.

## Differences from Qt

- While `ListView` does not implement `highlightRangeMode`, `preferredHighlightBegin`, or `preferredHighlightEnd`, `PathView` does support a strict highlight range.
- `TableView` does not support Shift or Control pointer range selection, does not select a block across reordered columns, and does not drag header sections to reorder them. `movableColumns` and `movableRows` are accepted as properties without effect.
- When `reuseItems` is `true`, `TableView` pools cell delegates that leave the viewport and emits the attached `pooled` and `reused` signals.

## See also

- [Models in QML](./models.md)
- [ListView and GridView](./list-and-grid-views.md)
- [Repeater, Instantiator, and DelegateChooser](./repeater-and-delegates.md)
