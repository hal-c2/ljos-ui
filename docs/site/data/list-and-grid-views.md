---
title: ListView and GridView
description: Display scrollable lists and grids that virtualise delegates, track a current item, and group rows into sections.
section: data
order: 2
---
# ListView and GridView

`ListView` and `GridView` display scrollable collections of rows or cells from a model. Unlike a `Repeater`, they virtualise their content, creating delegates only for the items near the visible viewport. Use `ListView` for vertical or horizontal lists with optional headers, footers, and sections, and `GridView` when you want items arranged in rows and columns of fixed cell sizes.

## Laying out a ListView

A `ListView` stacks its rows one after another, each as long as its delegate, with `spacing` placed between rows rather than around them. By default a `ListView` is vertical; setting `orientation` to horizontal places rows side by side. With `layoutDirection` set to right-to-left, the first row starts at the right edge; when oriented bottom-to-top, the first row starts at the bottom edge.

Rows can have different heights, and the total length of the list is estimated from the number of rows times the average height. If a row changes its height, the rows after it move accordingly. Prepending rows above the visible area or removing rows above the view does not shift what you currently see on screen.

A `ListView` can also display a `header` above the first row, a `footer` after the last row (accessible as `headerItem` and `footerItem`; removing the header destroys its item), a `highlight` that follows the current row, and section headers grouped by `section.property` and `section.criteria` (`ViewSection.FullString` or `ViewSection.FirstCharacter`). Each delegate can read its `index`, `model`, role names, and `required property` bindings, as well as the attached properties `ListView.view`, `ListView.isCurrentItem`, `ListView.section`, `ListView.previousSection`, and `ListView.nextSection`.

```qml
import QtQuick

ListView {
    width: 200
    height: 300
    model: ListModel {
        ListElement { name: "apple"; kind: "fruit" }
        ListElement { name: "banana"; kind: "fruit" }
        ListElement { name: "carrot"; kind: "root" }
    }
    header: Text { text: "Food"; height: 20 }
    footer: Text { text: "end"; height: 20 }
    section.property: "kind"
    section.delegate: Text {
        required property string section
        text: section
        height: 15
    }
    highlight: Rectangle { color: "#ffcc00" }
    delegate: Text {
        required property string name
        text: name
        height: 20
    }
}
```

The header sits above the rows, a 15-pixel section label appears before `"fruit"` and before `"root"`, and the highlight covers the current row. Notice that without `section.property` set, no sections are created.

## Laying out a GridView

A `GridView` fills cells into lines of `cellWidth` by `cellHeight`, placing as many cells on a line as fit before starting the next line. When `flow` is `GridView.FlowLeftToRight`, lines fill horizontally; when `flow` is `GridView.FlowTopToBottom`, it fills columns from top to bottom and scrolls sideways. With right-to-left or bottom-to-top layout, the first cell starts at that edge, and the `header` stays above the first line.

```qml
import QtQuick

GridView {
    width: 250
    height: 200
    cellWidth: 80
    cellHeight: 60
    focus: true
    model: 100
    highlight: Rectangle { color: "#ffcc00" }
    delegate: Rectangle {
        width: 70
        height: 50
        color: GridView.isCurrentItem ? "#ffcc00" : "#dddddd"
        Text { text: index }
    }
}
```

Three cells of 80 pixels fit across the 250-pixel width, and the arrow keys move the highlighted cell.

## Current item, highlights, and keyboard navigation

In both `ListView` and `GridView`, the first item (`currentIndex` `0`) is current at the start, and setting `currentIndex` to `-1` means no item is current. You can read the active delegate via `currentItem`.

When `highlightFollowsCurrentItem` is true, the `highlight` item (`highlightItem`) takes the position and size of the current row or cell, moving over `highlightMoveDuration` and scrolling the view if the new current item lies above or below the viewport. With `highlightMoveDuration` set to `0`, both the highlight and the view jump immediately. If `highlightFollowsCurrentItem` is false, the highlight stays where it is when `currentIndex` changes.

When the view has focus and `keyNavigationEnabled` is true:

- In a vertical `ListView`, `Down` and `Up` move the current row (`incrementCurrentIndex()` and `decrementCurrentIndex()`). In a horizontal `ListView`, `Right` and `Left` move the current row (and in a right-to-left list, `Left` advances).
- In a `GridView`, the arrow keys (`moveCurrentIndexUp()`, `moveCurrentIndexDown()`, `moveCurrentIndexLeft()`, `moveCurrentIndexRight()`) move between cells.
- Stepping stops at the first and last items unless `keyNavigationWraps` is true.
- The view's own `Keys` handler sees a key press before the view uses it; if your `Keys` handler accepts the key, the current row does not move.

You can also position or query the view from script using `positionViewAtIndex`, `positionViewAtBeginning`, `positionViewAtEnd`, `indexAt`, `itemAt`, `itemAtIndex`, and `forceLayout`, or snap scrolling to item boundaries with `snapMode` (`ListView.NoSnap`, `SnapToItem`, or `SnapOneItem`).

## Virtualisation, caching, and item reuse

`ListView` and `GridView` instantiate delegates only for the items near the visible area. Even with 100,000 rows in a `ListView` or 30,000 cells in a `GridView`, only a handful of delegates exist at any time, and scrolling far down destroys old delegates rather than letting them pile up.

```qml
import QtQuick

Item {
    id: root
    width: 200
    height: 300
    property int created: 0
    property int destroyed: 0
    readonly property int live: created - destroyed
    ListView {
        objectName: "list"
        anchors.fill: parent
        model: 100000
        delegate: Rectangle {
            width: 200
            height: 30
            Component.onCompleted: root.created++
            Component.onDestruction: root.destroyed++
            Text { text: "row " + index }
        }
    }
}
```

About ten to twenty-four delegates exist at once while showing this 100,000-row list.

You can tune virtualisation and transitions with these properties and attached signals:

- `cacheBuffer`, `displayMarginBeginning`, and `displayMarginEnd`: without a `cacheBuffer`, only the rows in the view exist. Rows created for the cache buffer are not drawn while they lie wholly outside the view and its display margins, whereas rows inside `displayMarginBeginning` or `displayMarginEnd` are drawn up to the margin edge.
- `reuseItems`: off by default. When set to `true`, scrolling recycles delegates through an internal pool instead of destroying and recreating them, emitting `ListView.pooled` (or `GridView.pooled`) when a delegate enters the pool and `reused` when it is reassigned to a new row. A pooled delegate still reports `visible: true`, but it is not drawn and receives no pointer input until it is taken from the pool again.
- Attached lifecycle signals and transitions: `ListView.onAdd` and `ListView.onRemove` run when the model adds or removes rows inside the view (rows the model already had at startup are not announced as added, and neither are rows outside the viewport). Setting `ListView.delayRemove` on a delegate keeps it alive until it releases the flag or its `remove` transition ends. You can also animate changes with `add`, `remove`, `move`, `displaced`, `addDisplaced`, `removeDisplaced`, and `moveDisplaced`.

## Differences from Qt

- `highlightRangeMode`, `preferredHighlightBegin`, `preferredHighlightEnd`, `section.labelPositioning`, and `Ljos.ListView.incubationBudgetMs` are not built.
- `reuseItems` defaults to `false`, matching Qt.
- When a script sets `currentIndex`, notifying the change after layout is not yet in `qml6` order.
- The number of instantiated delegates in a virtualised view falls within a band rather than an exact count, as Qt does not guarantee an exact number.

## See also

- [Models in QML](./models.md)
- [Repeater, Instantiator, and DelegateChooser](./repeater-and-delegates.md)
- [PathView, TableView, TreeView, and Flickable](./other-views.md)
