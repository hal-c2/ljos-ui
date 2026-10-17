# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html
#   https://doc.qt.io/qt-6/qml-qtquick-treeview.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Home, End and Tab in a TableView and a TreeView
  With a selectionModel, Home and End make the first and the last
  column of the current row current, with Control or without. Tab moves
  to the next cell, from the end of a row to the start of the next and
  from the last cell to the first. With nothing current each starts at
  the first row in sight. With Shift they select from where the Shift
  moves began. A view whose keyNavigationEnabled is false lets Tab move
  the focus on. Backtab, which goes the other way and never selects,
  and Tab with Shift are pinned by the QtTest file tst_table_keys.qml: a
  desktop types both as Shift+Tab, which each backend sends as one of
  them.

  Background:
    Given the file "Table.qml":
      """qml
      import QtQuick
      import QtQml.Models
      import Qt.labs.qmlmodels
      Item {
        width: 400; height: 300
        property alias view: tv
        TableView {
          id: tv; objectName: "tv"; width: 200; height: 200; focus: true
          property string out: ""
          selectionModel: ItemSelectionModel {}
          model: TableModel {
            TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" }
            rows: [ { a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 }, { a: 1, b: 2, c: 3 } ]
          }
          delegate: Item { implicitWidth: 50; implicitHeight: 20 }
          function go() {
            var s = [], all = selectionModel.selectedIndexes
            for (var i = 0; i < all.length; ++i) s.push(all[i].row + ":" + all[i].column)
            out = currentRow + " " + currentColumn + " [" + s.join(",") + "] " + activeFocus
          }
        }
        Rectangle { y: 250; width: 10; height: 10; activeFocusOnTab: true }
      }
      """

  Scenario Outline: <keys> in a table
    Given the QML:
      """qml
      import QtQuick
      Table {}
      """
    When the user presses "<a>"
    And the user presses "<b>"
    And the user presses "<c>"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | keys                   | a    | b      | c         | out               |
      | Home and End           | Down | Right  | Home      | 0 0 [] true       |
      | End                    | Down | Down   | End       | 1 2 [] true       |
      | Control and Home       | Down | Right  | Ctrl+Home | 0 0 [] true       |
      | Control and End        | Down | Down   | Ctrl+End  | 1 2 [] true       |
      | Home with none current | Home | Home   | Home      | 0 0 [] true       |
      | End with none current  | End  | End    | End       | 0 2 [] true       |
      | Tab with none current  | Tab  | Tab    | Tab       | 0 2 [] true       |
      | Tab past a row's end   | Down | End    | Tab       | 1 0 [] true       |
      | Shift and End          | Down | Down   | Shift+End | 1 2 [1:0,1:1,1:2] true |
      | Shift and Home         | Down | Right  | Shift+Home | 0 0 [0:0,0:1] true |

  Scenario: Tab past the last cell goes to the first
    Given the QML:
      """qml
      import QtQuick
      Table {}
      """
    When the user presses "Down"
    And the user presses "Down"
    And the user presses "Down"
    And the user presses "End"
    And the user presses "Tab"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "0 0 [] true"

  Scenario: Without key navigation Tab moves the focus on
    Given the QML:
      """qml
      import QtQuick
      Table { view.keyNavigationEnabled: false }
      """
    When the user presses "Tab"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "-1 -1 [] false"

  Scenario Outline: <keys> in a tree
    Given the QML:
      """qml
      import QtQuick
      import QtQml.Models
      import Qt.labs.qmlmodels
      TreeView {
        id: tv; objectName: "tv"; width: 200; height: 200; focus: true
        property string out: ""
        selectionModel: ItemSelectionModel {}
        model: TreeModel {
          TableModelColumn { display: "a" } TableModelColumn { display: "b" }
          rows: [ { a: 1, b: 2, rows: [ { a: 1, b: 2 } ] }, { a: 1, b: 2 } ]
        }
        delegate: Item { implicitWidth: 50; implicitHeight: 20 }
        function go() { out = currentRow + " " + currentColumn }
      }
      """
    When the user presses "<a>"
    And the user presses "<b>"
    And the user presses "<c>"
    And 50 milliseconds pass
    And the method "go" of "tv" is called
    Then the property "out" of "tv" is "<out>"

    Examples:
      | keys                 | a    | b    | c    | out |
      | Home with none       | Home | Home | Home | 0 0 |
      | End and Home         | Down | End  | Home | 0 0 |
      | End                  | Down | Down | End  | 1 1 |
      | Tab                  | Down | Tab  | Tab  | 1 0 |
