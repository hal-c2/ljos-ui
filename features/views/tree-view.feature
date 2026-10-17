# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qt-labs-qmlmodels-treemodel.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TreeView
  A TreeView shows a tree model as rows of a table: at first the rows at
  the top, and under a row that is expanded the rows under it. A delegate
  is made for each cell; the one in the first column is the tree node.
  What a row is in the tree reaches the delegate through required
  properties: depth, expanded, hasChildren, isTreeNode.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300
        property string heard: ""; property string out: ""
        onExpanded: (row, depth) => heard += "e" + row + ":" + depth + " "
        onCollapsed: (row, all) => heard += "c" + row + ":" + all + " "
        model: TreeModel {
          id: tm
          TableModelColumn { display: "name" }
          TableModelColumn { display: "size" }
          rows: [ { name: "docs", size: 2, rows: [ { name: "a", size: 1 }, { name: "sub", size: 1, rows: [ { name: "deep", size: 0 } ] } ] }, { name: "bin", size: 0 } ]
        }
        delegate: Item {
          implicitWidth: 60; implicitHeight: 20
          required property bool isTreeNode; required property bool expanded
          required property bool hasChildren; required property int depth
          required property string display
        }
        function cells() { var s = []; for (var r = 0; r < rows; ++r) { var c = itemAtCell(Qt.point(0, r)); s.push(c.display + c.depth + (c.expanded ? "+" : "") + (c.hasChildren ? ">" : "")) } out = s.join(" ") }
        function indexes() { var i = index(2, 0); out = [i.row, i.parent.row, tm.data(i), rowAtIndex(i), cellAtIndex(i).y, depth(2), isExpanded(0), isExpanded(1), tm.data(modelIndex(Qt.point(1, 2)))].join(" ") }
        function nodes() { out = [itemAtCell(Qt.point(0, 0)).isTreeNode, itemAtCell(Qt.point(1, 0)).isTreeNode, itemAtCell(Qt.point(1, 0)).display, columns].join(" ") }
        function reach() { expandToIndex(tm.index([0, 1, 0], 0)) }
        function grow() { tm.appendRow(tm.index([0], 0), { name: "z", size: 0 }) }
      }
      """

  Scenario: At first only the rows at the top are shown
    When 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0> bin0"
    And the property "rows" of "tv" is 2
    And the property "columns" of "tv" is 2

  Scenario: Every cell has a delegate and only the first column is the tree node
    When 50 milliseconds pass
    And the method "nodes" of "tv" is called
    Then the property "out" of "tv" is "true false 2 2"

  Scenario: Expanding a row shows the rows under it
    When the method "expand" of "tv" is called with 0
    Then the property "heard" of "tv" is "e0:1 "
    When 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a1 sub1> bin0"
    And the property "rows" of "tv" is 4

  Scenario: Collapsing a row hides the rows under it
    When the method "expand" of "tv" is called with 0
    And 50 milliseconds pass
    And the method "collapse" of "tv" is called with 0
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0> bin0"
    And the property "heard" of "tv" is "e0:1 c0:false "

  Scenario: toggleExpanded expands a collapsed row and collapses an expanded one
    When the method "toggleExpanded" of "tv" is called with 0
    And 50 milliseconds pass
    Then the property "rows" of "tv" is 4
    When the method "toggleExpanded" of "tv" is called with 0
    And 50 milliseconds pass
    Then the property "rows" of "tv" is 2

  Scenario: expandRecursively expands every row, collapseRecursively collapses them
    When the method "expandRecursively" of "tv" is called
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a1 sub1+> deep2 bin0"
    When the method "collapseRecursively" of "tv" is called
    And 50 milliseconds pass
    Then the property "rows" of "tv" is 2
    And the property "heard" of "tv" is "e-1:-1 c-1:true "

  Scenario: expandToIndex expands the rows above an item so it is shown
    When the method "reach" of "tv" is called
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a1 sub1+> deep2 bin0"
    And the property "heard" of "tv" is "e0:2 "

  Scenario: A row without rows under it is expanded, and no rows come
    When 50 milliseconds pass
    And the method "expand" of "tv" is called with 1
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0> bin0+"
    And the property "heard" of "tv" is "e1:1 "

  Scenario: Expanding a row that is expanded already is not heard again
    When the method "expand" of "tv" is called with 0
    And the method "expand" of "tv" is called with 0
    Then the property "heard" of "tv" is "e0:1 "

  Scenario: A row of the view and an index of the model name each other
    When the method "expand" of "tv" is called with 0
    And 50 milliseconds pass
    And the method "indexes" of "tv" is called
    Then the property "out" of "tv" is "1 0 sub 2 2 1 true false 1"

  Scenario: A row added under an expanded row is shown
    When the method "expand" of "tv" is called with 0
    And 50 milliseconds pass
    And the method "grow" of "tv" is called
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a1 sub1> z1 bin0"
