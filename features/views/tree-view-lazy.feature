# Sources:
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#hasChildren
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#canFetchMore
#   https://doc.qt.io/qt-6/qabstractitemmodel.html#fetchMore
# The model is one the application writes in Rust (ItemTree with
# has_children, can_fetch_more and fetch_more), as a file system or a
# remote listing is. Qt's counterpart is a C++ model, which no step can
# give the Qt backend, so the scenarios run on Ljós only.

@ljos-only
Feature: A TreeView of a tree model that reads its rows lazily
  A tree model may say that a row has rows under it before it has read
  them. The TreeView shows such a row with its expand arrow, and asks the
  model for the rows when the row is expanded, alone or with the rows
  under it. The rows come when the model has read them, as rows inserted
  under the row.

  Background:
    Given a tree model "files" that reads a folder's rows 100 ms after a view asks for them:
      | folder | name  |
      |        | docs  |
      |        | bin   |
      | docs   | a.txt |
      | docs   | b.txt |
    And the QML:
      """qml
      import QtQuick
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 300
        property string out: ""
        model: files
        delegate: Item {
          implicitWidth: 60; implicitHeight: 20
          required property bool expanded; required property bool hasChildren
          required property int depth; required property string display
        }
        function cells() { var s = []; for (var r = 0; r < rows; ++r) { var c = itemAtCell(Qt.point(0, r)); s.push(c.display + c.depth + (c.expanded ? "+" : "") + (c.hasChildren ? ">" : "")) } out = s.join(" ") }
      }
      """

  Scenario: A folder that is not read yet shows its expand arrow
    When 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0> bin0"

  Scenario: Expanding a folder that is not read yet fills it in once it is read
    When the method "expand" of "tv" is called with 0
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> bin0"
    When 100 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a.txt1 b.txt1 bin0"

  Scenario: A folder is read once
    When the method "expand" of "tv" is called with 0
    And 150 milliseconds pass
    And the method "collapse" of "tv" is called with 0
    And the method "expand" of "tv" is called with 0
    And 50 milliseconds pass
    Then the property "rows" of "tv" is 4

  Scenario: Expanding every row asks for the rows of each folder not read yet
    When the method "expandRecursively" of "tv" is called
    And 50 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> bin0"
    When 100 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a.txt1 b.txt1 bin0"

  Scenario: Expanding a row and the rows under it asks for its rows
    When the method "expandRecursively" of "tv" is called with 0, 1
    And 150 milliseconds pass
    And the method "cells" of "tv" is called
    Then the property "out" of "tv" is "docs0+> a.txt1 b.txt1 bin0"
