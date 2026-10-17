# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#moveColumn-method
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#moveRow-method
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#columnMoved-signal
# Verified on Qt 6.11.2 (qml6, offscreen). What a model change does after
# a move is not pinned: qml6 aborts on a row appended then.

@qt-parity
Feature: Reordering a TableView's columns and rows
  moveColumn and moveRow show a column or row of the view at another
  place, the ones between shifting over; the model keeps its order. A
  cell is the view's: itemAtCell and index() take the view's column and
  row, cellAtIndex gives them, and a delegate's row and column are the
  model's. Each column (row) that moved is told by columnMoved (rowMoved)
  with its model index, where it was and where it is, in the model's
  order. A move out of range or in place does nothing. The sizes stay
  with the places: a size provider is asked by the view's column.
  clearColumnReordering and clearRowReordering put the model's order
  back and tell nothing.

  Background:
    Given the QML:
      """qml
      import QtQuick; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "o"; width: 300; height: 200
        property string moves: ""; property string grid: ""; property string widths: ""; property string mapped: ""
        function cell(x, y) { const d = tv.itemAtCell(x, y); return d ? d.text : "-" }
        function look() {
          grid = [0, 1, 2].map(y => [0, 1, 2].map(x => cell(x, y)).join(" ")).join(" | ")
          const a = tv.itemAtCell(0, 0), c = tv.itemAtCell(2, 0)
          widths = a.x + "/" + a.width + " " + c.x + "/" + c.width
          const i = tv.index(0, 0), p = tv.cellAtIndex(tm.index(0, 0))
          mapped = i.row + "," + i.column + " " + p.x + "," + p.y
        }
        TableView {
          id: tv; objectName: "tv"; anchors.fill: parent
          columnWidthProvider: c => 50 + c * 10; rowHeightProvider: r => 20
          model: TableModel { id: tm
            TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" }
            rows: [{a: "a0", b: "b0", c: "c0"}, {a: "a1", b: "b1", c: "c1"}, {a: "a2", b: "b2", c: "c2"}]
          }
          delegate: Text { required property string display; required property int row; required property int column; text: display + row + column }
          onColumnMoved: (l, o, n) => root.moves += "c" + l + o + n + ";"
          onRowMoved: (l, o, n) => root.moves += "r" + l + o + n + ";"
        }
      }
      """

  Scenario: A column moved to the end
    When the method "moveColumn" of "tv" is called with 0, 2
    And 50 milliseconds pass
    And the method "look" of "o" is called
    Then the property "moves" of "o" is "c002;c110;c221;"
    And the property "grid" of "o" is "b001 c002 a000 | b111 c112 a110 | b221 c222 a220"
    And the property "widths" of "o" is "0/50 110/70"
    And the property "mapped" of "o" is "0,1 2,0"

  Scenario: A row moved to the top
    When the method "moveRow" of "tv" is called with 2, 0
    And 50 milliseconds pass
    And the method "look" of "o" is called
    Then the property "moves" of "o" is "r001;r112;r220;"
    And the property "grid" of "o" is "a220 b221 c222 | a000 b001 c002 | a110 b111 c112"
    And the property "mapped" of "o" is "2,0 0,1"

  Scenario Outline: moveColumn(<from>, <to>) does nothing
    When the method "moveColumn" of "tv" is called with <from>, <to>
    And 50 milliseconds pass
    And the method "look" of "o" is called
    Then the property "moves" of "o" is ""
    And the property "grid" of "o" is "a000 b001 c002 | a110 b111 c112 | a220 b221 c222"

    Examples:
      | from | to |
      | 1    | 1  |
      | 5    | 0  |
      | 0    | 9  |
      | -1   | 0  |

  Scenario: Clearing the reordering puts the model's order back
    When the method "moveColumn" of "tv" is called with 0, 2
    And the method "moveRow" of "tv" is called with 0, 1
    And 50 milliseconds pass
    And the property "moves" of "o" is set to ""
    And the method "clearColumnReordering" of "tv" is called
    And the method "clearRowReordering" of "tv" is called
    And 50 milliseconds pass
    And the method "look" of "o" is called
    Then the property "moves" of "o" is ""
    And the property "grid" of "o" is "a000 b001 c002 | a110 b111 c112 | a220 b221 c222"
