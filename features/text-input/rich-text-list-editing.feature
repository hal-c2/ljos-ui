# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#textFormat-prop
#   https://doc.qt.io/qt-6/richtext-html-subset.html
# Verified on Qt 6.11.2 (qml6, offscreen and vnc).

@qt-parity
Feature: Return and Backspace in the items of a list in a rich TextEdit
  Return in a list item starts another item of the list. Backspace at the
  start of an item does not join it to the paragraph before at once: the
  first press takes the item out of its list, so that it loses its bullet
  and keeps its indent, the second takes the indent away, and only the
  third joins it. The list has the items "three" and "four", and a
  paragraph "five" follows it. Each paragraph is shown as its text and
  where it starts, at 14 pixels in Liberation Sans.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"; width: 400; height: 300; color: "white"
        property string out
        function at(i) { edit.forceActiveFocus(); edit.cursorPosition = i }
        function all() {
          const t = edit.getText(0, edit.length)
          let s = 0; const shown = []
          for (const line of t.split(/[\n\u2029]/)) {
            const r = edit.positionToRectangle(s)
            shown.push(line + "@" + Math.round(r.x) + "," + Math.round(r.y))
            s += line.length + 1
          }
          out = edit.length + " c" + edit.cursorPosition + " " + shown.join("|")
        }
        TextEdit {
          id: edit; objectName: "edit"; width: 300
          textFormat: TextEdit.RichText; font.family: "Liberation Sans"; font.pixelSize: 14
          text: "<ul><li>three</li><li>four</li></ul><p>five</p>"
        }
      }
      """

  Rule: Return starts another item

    Scenario Outline: Return at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Return"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 2  | 16 c3 th@40,0\|ree@40,28\|four@40,44\|five@0,72 |
        | 5  | 16 c6 three@40,0\|@40,28\|four@40,44\|five@0,72 |

    Scenario Outline: Return twice at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Return"
      And the user presses "Return"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 2  | 17 c4 th@40,0\|@40,28\|ree@40,56\|four@40,72\|five@0,100 |

  Rule: Backspace at an item's start takes it out of the list first

    Scenario Outline: Backspace at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Backspace"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 6  | 15 c6 three@40,0\|four@40,16\|five@0,44 |
        | 11 | 14 c10 three@40,0\|fourfive@40,16 |

    Scenario Outline: Backspace twice at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Backspace"
      And the user presses "Backspace"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 6  | 15 c6 three@40,0\|four@0,16\|five@0,44 |

    Scenario Outline: Backspace three times at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Backspace"
      And the user presses "Backspace"
      And the user presses "Backspace"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 6  | 14 c5 threefour@40,0\|five@0,28 |
        | 0  | 15 c0 three@0,0\|four@40,16\|five@0,44 |

    Scenario Outline: Return then Backspace at <at> leaves <text>
      When the method "at" of "root" is called with <at>
      And the user presses "Return"
      And the user presses "Backspace"
      And the method "all" of "root" is called
      Then the property "out" of "root" is "<text>"

      Examples:
        | at | text |
        | 2  | 16 c3 th@40,0\|ree@40,28\|four@40,44\|five@0,72 |
        | 5  | 16 c6 three@40,0\|@40,28\|four@40,44\|five@0,72 |

    Scenario: An item has its bullet
      When the method "at" of "root" is called with 6
      Then the pixel at 32, 24 of the window is within 8 of "#020202"

    Scenario: An item out of its list has no bullet
      When the method "at" of "root" is called with 6
      And the user presses "Backspace"
      Then the pixel at 32, 24 of the window is within 8 of "#ffffff"
