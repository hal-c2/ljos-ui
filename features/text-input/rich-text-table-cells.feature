# Sources:
#   https://doc.qt.io/qt-6/richtext-html-subset.html#table-attributes
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionToRectangle-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Images and right-to-left text in the cells of a rich TextEdit
  An image in a cell makes its row as high as the image, and the cursor
  after it stands at its right, even where the cell ends with it, as it
  does where an image ends the text. A cell that reads right to left
  stands at the right of its column: its first position is at the right
  of its text and its last at the left. The text is Liberation Sans at 20
  pixels, where a "c" is ten pixels wide and a line 23 high; the image is
  30 by 60.

  Background:
    Given a 30 by 60 image file "tall.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 400; height: 400; color: "white"
        property string doc
        property string out
        function place(i) { const r = edit.positionToRectangle(i); return r.x + "," + r.y + "," + r.height }
        function measure(a, b) {
          const p = []
          for (let i = a; i <= b; i++) p.push(place(i))
          out = edit.implicitWidth + "x" + edit.implicitHeight + " " + p.join(" ")
        }
        function at(x, y) { out = "" + edit.positionAt(x, y) }
        TextEdit {
          id: edit; objectName: "edit"
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 20
          text: root.doc.split("[").join("<").split("]").join(">")
        }
      }
      """

  Rule: An image stands in its cell

    Scenario Outline: <doc> measures <out> from <from> to <to>
      When the property "doc" of "root" is set to "<doc>"
      And 100 milliseconds pass
      And the method "measure" of "root" is called with <from>, <to>
      Then the property "out" of "root" is "<out>"

      Examples:
        | doc | from | to | out |
        | [table border='1'][tr][td][img src='tall.png'][/td][td]c[/td][/tr][/table] | 1 | 4 | 52x68 4,4,60 34,4,60 38,4,23 48,4,23 |
        | [img src='tall.png'] | 0 | 1 | 30x60 0,0,60 30,0,60 |

  Rule: A cell that reads right to left stands at the right of its column

    Scenario Outline: The positions of <doc> from <from> to <to> are <out>
      When the property "doc" of "root" is set to "<doc>"
      And the method "measure" of "root" is called with <from>, <to>
      Then the property "out" of "root" is "<out>"

      Examples:
        | doc | from | to | out |
        | [table][tr][td]אב[/td][/tr][tr][td]cccccccccc[/td][/tr][/table] | 1 | 4 | 104x52 102,2,23 89.453125,2,23 77.5,2,23 2,27,23 |
        | [table border='1'][tr][td]אב[/td][td]cc[/td][/tr][/table] | 1 | 4 | 57x31 29,4,23 16.453125,4,23 4.5,4,23 33,4,23 |

    Scenario Outline: A click at <x>, 10 is position <pos>
      When the property "doc" of "root" is set to "[table][tr][td]אב[/td][/tr][tr][td]cccccccccc[/td][/tr][/table]"
      And the method "at" of "root" is called with <x>, 10
      Then the property "out" of "root" is "<pos>"

      Examples:
        | x  | pos |
        | 10 | 3   |
        | 95 | 2   |
        | 103 | 1  |
