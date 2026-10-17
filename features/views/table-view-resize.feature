# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#setColumnWidth-method
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#resizableColumns-prop
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#resizableRows-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Resizing a TableView's columns and rows
  setColumnWidth and setRowHeight give a column or a row an explicit
  size, which the next layout takes instead of the delegates' implicit
  one: 0 hides it, and a negative size takes the explicit size away. A
  columnWidthProvider or rowHeightProvider still wins. A view synced to
  another shares the explicit sizes of the synced directions with it.
  With resizableColumns (resizableRows) the user drags the end of a
  column (row) to resize it: the press is the view's within 4 of the
  end, or in the spacing after it, and the size follows the pointer
  from where the drag began, but not below 0.001; the view does not
  flick meanwhile. A header view resizes its own sections, and so the
  view it heads.

  Background:
    Given the file "Sized.qml":
      """qml
      import QtQuick; import QtQuick.Controls; import Qt.labs.qmlmodels
      Item {
        id: root; objectName: "root"; width: 400; height: 300
        property alias tv: tv; property string out: ""
        property alias resizableColumns: tv.resizableColumns; property alias resizableRows: tv.resizableRows
        property alias columnSpacing: tv.columnSpacing
        TableView {
          id: tv; width: 300; height: 200
          model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" } TableModelColumn { display: "c" } TableModelColumn { display: "d" }
            rows: { var r = []; for (var i = 0; i < 20; ++i) r.push({ a: 0, b: 0, c: 0, d: 0 }); return r } }
          delegate: Rectangle { implicitWidth: 100; implicitHeight: 30 }
        }
        function sizes() { out = [tv.columnWidth(0), tv.columnWidth(1), tv.columnWidth(2), tv.columnWidth(3), tv.rowHeight(0), tv.rowHeight(1)].join(",") }
        function explicit() { out = [tv.explicitColumnWidth(0), tv.explicitColumnWidth(1), tv.explicitColumnWidth(9), tv.explicitRowHeight(0)].join(",") }
        function at() { out = [tv.contentX, tv.contentY].join(",") }
        function wide() { out = tv.contentWidth }
        function clear() { tv.clearColumnWidths(); tv.clearRowHeights() }
      }
      """

  Scenario: Explicit sizes are laid out, and cleared
    Given the QML:
      """qml
      import QtQuick
      Sized { function size() { tv.setColumnWidth(1, 40); tv.setRowHeight(0, 60); tv.setColumnWidth(9, 33) } }
      """
    When 50 milliseconds pass
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,-1,-1,-1"
    When the method "size" of "root" is called
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,40,33,60"
    When 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,40,100,100,60,30"
    When the method "wide" of "root" is called
    Then the property "out" of "root" is "340"
    When the method "clear" of "root" is called
    And 50 milliseconds pass
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,-1,-1,-1"
    When the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,100,100,-1,30,30"

  Scenario: A width of 0 hides the column, and a negative one takes the explicit width away
    Given the QML:
      """qml
      import QtQuick
      Sized { function hide() { tv.setColumnWidth(1, 0) } function unset() { tv.setColumnWidth(1, -5) } }
      """
    When 50 milliseconds pass
    And the method "hide" of "root" is called
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,-1,100,100,30,30"
    When the method "wide" of "root" is called
    Then the property "out" of "root" is "300"
    When the method "unset" of "root" is called
    And 50 milliseconds pass
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,-1,-1,-1"
    When the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,100,100,-1,30,30"

  Scenario: A size provider wins over an explicit size, which stays
    Given the QML:
      """qml
      import QtQuick
      Sized {
        function size() { tv.setColumnWidth(1, 40) }
        function provide() { tv.columnWidthProvider = function (c) { return c === 1 ? 70 : -1 } }
        function unprovide() { tv.columnWidthProvider = undefined }
      }
      """
    When 50 milliseconds pass
    And the method "size" of "root" is called
    And the method "provide" of "root" is called
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,70,100,100,30,30"
    When the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,40,-1,-1"
    When the method "unprovide" of "root" is called
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,40,100,100,30,30"

  Scenario: The implicit size of a column or row is its delegates', while it is loaded
    Given the QML:
      """qml
      import QtQuick
      Sized { function implicit() { out = [tv.implicitColumnWidth(0), tv.implicitColumnWidth(3), tv.implicitRowHeight(0), tv.implicitRowHeight(19)].join(",") } }
      """
    When 50 milliseconds pass
    And the method "implicit" of "root" is called
    Then the property "out" of "root" is "100,-1,30,-1"

  Scenario Outline: Dragging the end of a column at <x> resizes it
    Given the QML:
      """qml
      import QtQuick
      Sized { resizableColumns: true }
      """
    When 50 milliseconds pass
    And the user moves the pointer to <before>, 45
    And the user moves the pointer to <x>, 45
    And the user presses the pointer at <x>, 45
    And the user moves the pointer to <past>, 45
    And the user moves the pointer to <to>, 45
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "125,100,100,-1,30,30"
    When the user releases the pointer at <to>, 45
    And 50 milliseconds pass
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "125,-1,-1,-1"
    When the method "at" of "root" is called
    Then the property "out" of "root" is "0,0"

    Examples:
      | x   | before | past | to  |
      | 100 | 99     | 115  | 140 |
      | 96  | 95     | 111  | 136 |
      | 104 | 103    | 119  | 144 |

  Scenario Outline: A press at <x>, away from the end of a column, does not resize it
    Given the QML:
      """qml
      import QtQuick
      Sized { resizableColumns: <resizable> }
      """
    When 50 milliseconds pass
    And the user moves the pointer to <before>, 45
    And the user moves the pointer to <x>, 45
    And the user presses the pointer at <x>, 45
    And the user moves the pointer to <past>, 45
    And the user moves the pointer to <to>, 45
    And the user releases the pointer at <to>, 45
    And 1000 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,100,100,-1,30,30"
    When the method "explicit" of "root" is called
    Then the property "out" of "root" is "-1,-1,-1,-1"

    Examples:
      | x   | before | past | to  | resizable |
      | 95  | 94     | 110  | 135 | true      |
      | 105 | 104    | 120  | 145 | true      |
      | 3   | 2      | 18   | 43  | true      |
      | 100 | 99     | 115  | 140 | false     |

  Scenario: A press in the spacing after a column is on its end
    Given the QML:
      """qml
      import QtQuick
      Sized { resizableColumns: true; columnSpacing: 10 }
      """
    When 50 milliseconds pass
    And the user moves the pointer to 113, 45
    And the user moves the pointer to 114, 45
    And the user presses the pointer at 114, 45
    And the user moves the pointer to 130, 45
    And the user moves the pointer to 155, 45
    And the user releases the pointer at 155, 45
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "125,100,100,-1,30,30"

  Scenario: Dragging the end of a row resizes it, and a drag across it does nothing
    Given the QML:
      """qml
      import QtQuick
      Sized { resizableRows: true }
      """
    When 50 milliseconds pass
    And the user moves the pointer to 50, 29
    And the user moves the pointer to 50, 30
    And the user presses the pointer at 50, 30
    And the user moves the pointer to 50, 45
    And the user moves the pointer to 50, 70
    And the user releases the pointer at 50, 70
    And 50 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,100,100,-1,55,30"
    When the user moves the pointer to 50, 84
    And the user moves the pointer to 50, 85
    And the user presses the pointer at 50, 85
    And the user moves the pointer to 70, 85
    And the user moves the pointer to 110, 85
    And the user releases the pointer at 110, 85
    And 1000 milliseconds pass
    And the method "sizes" of "root" is called
    Then the property "out" of "root" is "100,100,100,-1,55,30"
    When the method "at" of "root" is called
    Then the property "out" of "root" is "0,0"

  Scenario: A column dragged narrower than nothing is 0.001 wide
    Given the QML:
      """qml
      import QtQuick
      Sized { resizableColumns: true; tv.x: 50 }
      """
    When 50 milliseconds pass
    And the user moves the pointer to 149, 45
    And the user moves the pointer to 150, 45
    And the user presses the pointer at 150, 45
    And the user moves the pointer to 135, 45
    And the user moves the pointer to 10, 45
    And the user releases the pointer at 10, 45
    And 50 milliseconds pass
    And the method "explicit" of "root" is called
    Then the property "out" of "root" is "0.001,-1,-1,-1"

  Scenario: Header views resize the sections of the view they head
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls
      Sized {
        tv.x: 50; tv.y: 30
        HorizontalHeaderView { id: hh; x: 50; syncView: tv; model: ["A", "B", "C", "D"] }
        VerticalHeaderView { id: vh; y: 30; syncView: tv }
        function heads() { out = [hh.resizableColumns, hh.resizableRows, vh.resizableColumns, vh.resizableRows, tv.resizableColumns].join(" ") }
        function both() { out = [tv.columnWidth(0), hh.columnWidth(0), tv.explicitColumnWidth(0), hh.explicitColumnWidth(0),
                                 tv.rowHeight(0), vh.rowHeight(0), tv.explicitRowHeight(0), vh.explicitRowHeight(0)].join(",") }
        function size() { tv.clearColumnWidths(); hh.setColumnWidth(0, 70) }
      }
      """
    When 50 milliseconds pass
    And the method "heads" of "root" is called
    Then the property "out" of "root" is "true false false true false"
    When the user moves the pointer to 149, 10
    And the user moves the pointer to 150, 10
    And the user presses the pointer at 150, 10
    And the user moves the pointer to 165, 10
    And the user moves the pointer to 190, 10
    And the user releases the pointer at 190, 10
    And the user moves the pointer to 20, 59
    And the user moves the pointer to 20, 60
    And the user presses the pointer at 20, 60
    And the user moves the pointer to 20, 75
    And the user moves the pointer to 20, 100
    And the user releases the pointer at 20, 100
    And 50 milliseconds pass
    And the method "both" of "root" is called
    Then the property "out" of "root" is "125,125,125,125,55,55,55,55"
    When the method "size" of "root" is called
    And 50 milliseconds pass
    And the method "both" of "root" is called
    Then the property "out" of "root" is "70,70,70,70,55,55,55,55"
