# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#resizableColumns-prop
#   https://doc.qt.io/qt-6/qml-qtquick-tableview.html#resizableRows-prop
# Verified on Qt 6.11.2 (qml6 under X11, the cursor read with XFixes): a
# column end that can be resized shows Qt.SplitHCursor, a row end
# Qt.SplitVCursor, a cell the arrow. The offscreen oracle has no pointer
# to show a cursor.

@ljos-only
Feature: The pointer over the end of a TableView's column or row
  Over the end of a column the user may resize the pointer shows
  Qt.SplitHCursor, over the end of such a row Qt.SplitVCursor, and the
  arrow elsewhere. While the user drags an end the cursor stays.

  Background:
    Given the QML:
      """qml
      import QtQuick; import Qt.labs.qmlmodels
      TableView {
        objectName: "tv"; width: 300; height: 200
        resizableColumns: true; resizableRows: true
        model: TableModel { TableModelColumn { display: "a" } TableModelColumn { display: "b" }
          rows: [{ a: 0, b: 0 }, { a: 0, b: 0 }, { a: 0, b: 0 }] }
        delegate: Rectangle { implicitWidth: 100; implicitHeight: 30 }
      }
      """

  Scenario Outline: At <x>, <y> the pointer shows the <cursor>
    When 50 milliseconds pass
    And the user moves the pointer to <x>, <y>
    Then the pointer shows the "<cursor>" cursor

    Examples:
      | x   | y  | cursor        |
      | 50  | 15 | ArrowCursor   |
      | 99  | 15 | SplitHCursor  |
      | 103 | 15 | SplitHCursor  |
      | 50  | 30 | SplitVCursor  |

  Scenario: The cursor stays while an end is dragged
    When 50 milliseconds pass
    And the user moves the pointer to 100, 15
    And the user presses the pointer at 100, 15
    And the user moves the pointer to 140, 15
    Then the pointer shows the "SplitHCursor" cursor
    When the user releases the pointer at 140, 15
    And the user moves the pointer to 120, 15
    Then the pointer shows the "ArrowCursor" cursor
