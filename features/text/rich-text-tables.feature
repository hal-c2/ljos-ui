# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-attributes
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-cell-attributes
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Tables in a rich Text
  A table's bgcolor fills the whole table, the spacing between its cells
  too, and a cell's bgcolor, or its background-color, fills the cell
  inside its padding. The frames around the table and around each cell
  are as wide as the border and grey, whatever colour the text is. A
  table given less width than its text needs wraps the text in its cells,
  and a cell that reads right to left stands at the right of its column.
  The text is Liberation Sans, where at 36 pixels a no-break space is ten
  pixels wide and a line 41 high, and at 20 pixels a "c" is ten wide.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"; width: 400; height: 300; color: "white"
        Text {
          textFormat: Text.RichText; font.family: "Liberation Sans"; font.pixelSize: 36; color: "#0000ff"
          text: "<table border='3' cellspacing='4' cellpadding='2' bgcolor='#ff0000'><tr><td bgcolor='#00ff00'>&nbsp;&nbsp;</td><td>&nbsp;</td></tr><tr><td bgcolor='#00ff00'>&nbsp;</td><td style='background-color:#ffff00'>&nbsp;</td></tr></table>"
        }
        Text {
          objectName: "wrapped"; x: 200; width: 80; wrapMode: Text.WordWrap
          textFormat: Text.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "<table border='1'><tr><td>cc cc cc cc</td><td>c c</td></tr></table>"
        }
        Text {
          y: 200; textFormat: Text.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: "<table><tr><td><span style='background-color:#00ff00'>\u05d0\u05d1</span></td></tr><tr><td>cccccccccc</td></tr></table>"
        }
      }
      """

  Rule: Backgrounds fill the table and the cells

    Scenario Outline: The pixel at <x>, <y> is <colour>, <where>
      When 100 milliseconds pass
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y  | colour  | where                                 |
        | 20 | 30 | #00ff00 | in a cell with a bgcolor              |
        | 31 | 52 | #00ff00 | at the corner of that cell's text     |
        | 11 | 30 | #ff0000 | in that cell's padding                |
        | 20 | 60 | #ff0000 | between two rows                      |
        | 39 | 30 | #ff0000 | between two columns                   |
        | 50 | 30 | #ff0000 | in a cell with no bgcolor             |
        | 50 | 85 | #ffff00 | in a cell with a background-color     |
        | 70 | 30 | #ffffff | outside the table                     |

  Rule: Frames are grey and as wide as the border

    Scenario Outline: The pixel at <x>, <y> is <colour>, <where>
      When 100 milliseconds pass
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y  | colour  | where                                 |
        | 1  | 30 | #808080 | on the table's frame                  |
        | 66 | 30 | #808080 | on the far side of the table's frame  |
        | 8  | 30 | #808080 | on a cell's frame                     |
        | 20 | 57 | #808080 | on the third pixel of that frame      |
        | 20 | 64 | #808080 | on the frame of the cell below        |

  Rule: Cells wrap in a table given too little width

    Scenario: The columns share out the width the table is given
      Then the property "contentWidth" of "wrapped" is 80
      And the property "implicitWidth" of "wrapped" is 135
      And the property "contentHeight" of "wrapped" is 100

  Rule: A cell that reads right to left stands at the right of its column

    Scenario Outline: The pixel at <x>, <y> is <colour>, <where>
      When 100 milliseconds pass
      Then the pixel at <x>, <y> of the window is "<colour>"

      Examples:
        | x  | y   | colour  | where                                    |
        | 10 | 212 | #ffffff | at the left of the column                |
        | 90 | 212 | #00ff00 | under the text, at the right of the column |
