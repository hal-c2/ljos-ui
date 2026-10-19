# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-cell-attributes
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionToRectangle-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Tables in a rich TextEdit
  A table in rich text stands in a TextEdit as it does in a rich Text: its
  cells in columns as wide as their widest text and rows as high as their
  line, apart by the border, the spacing between cells and each cell's
  frame. The cursor stands in the cells where their text is, a click puts
  it in the cell under the pointer, and up and down go to the row above
  or below. The text is Liberation Sans at 20 pixels, where a "c" is ten
  pixels wide and a line 23 high, and the table stands between two
  paragraphs, where qml6 counts positions in it as Ljós does.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "window"
        width: 400; height: 300
        property string places: ""
        TextEdit {
          id: edit; objectName: "edit"
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "x<table border='1'><tr><td>ccc</td><td>cc</td></tr><tr><td>c</td><td>cccc</td></tr></table>y"
        }
        Component.onCompleted: {
          for (const i of [2, 5, 6, 9, 11, 15, 16]) {
            const r = edit.positionToRectangle(i)
            places += (places ? " " : "") + i + ":" + r.x + "," + r.y
          }
        }
      }
      """

  Rule: The cells stand in a grid

    Scenario: The text of every cell starts in its column and its row
      Then the property "places" of "window" is "2:4,27 5:34,27 6:38,27 9:4,54 11:38,54 15:78,54 16:0,81"

    Scenario: The editor is as large as the table and the paragraphs around it
      Then the property "implicitWidth" of "edit" is 82
      And the property "implicitHeight" of "edit" is 104

  Rule: The cursor goes from cell to cell

    Scenario Outline: A click at <x>, <y> puts the cursor at <position>
      When the user presses the pointer at <x>, <y>
      And the user releases the pointer at <x>, <y>
      Then the property "cursorPosition" of "edit" is <position>

      Examples:
        | x  | y  | position |
        | 60 | 60 | 13       |
        | 34 | 34 | 5        |
        | 2  | 34 | 2        |
        | 50 | 34 | 7        |
        | 20 | 23 | 1        |
        | 20 | 25 | 2        |
        | 50 | 25 | 6        |

    Scenario: Down goes to the cell below
      When the user presses the pointer at 20, 34
      And the user releases the pointer at 20, 34
      And the user presses "Down"
      Then the property "cursorPosition" of "edit" is 10

    Scenario: Up goes to the cell above
      When the user presses the pointer at 50, 60
      And the user releases the pointer at 50, 60
      And the user presses "Up"
      Then the property "cursorPosition" of "edit" is 7
