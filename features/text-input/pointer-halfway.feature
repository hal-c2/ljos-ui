# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#positionAt-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#positionAt-method
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

@qt-parity
Feature: A press halfway across a character
  A press puts the cursor at the nearer edge of the character under it.
  Exactly halfway across, the cursor goes before the character, in a
  TextEdit and a TextInput alike, and so does a drag that ends there. In
  Liberation Sans at 20 pixels an "x" is 10 pixels wide.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 300; height: 200
        property string out
        function shown() { out = edit.selectionStart + "-" + edit.selectionEnd + " c" + edit.cursorPosition }
        TextEdit {
          id: edit; objectName: "edit"
          font.family: "Liberation Sans"; font.pixelSize: 20
          text: "xx\nyy"
        }
        TextInput {
          objectName: "field"; y: 100
          font.family: "Liberation Sans"; font.pixelSize: 20
          text: "xx"
        }
      }
      """

  Rule: Halfway is before the character

    Scenario Outline: A press at <x>, <y> in a TextEdit puts the cursor at <position>
      When the user presses the pointer at <x>, <y>
      And the user releases the pointer at <x>, <y>
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "<position>-<position> c<position>"

      Examples:
        | x  | y  | position |
        | 4  | 10 | 0        |
        | 5  | 10 | 0        |
        | 6  | 10 | 1        |
        | 15 | 10 | 1        |
        | 5  | 30 | 3        |

    Scenario Outline: A drag from 18, 10 to <x>, 10 selects <shown>
      When the user drags from 18, 10 to <x>, 10
      And the method "shown" of "root" is called
      Then the property "out" of "root" is "<shown>"

      Examples:
        | x  | shown  |
        | 5  | 0-2 c0 |
        | 15 | 1-2 c1 |

    Scenario Outline: A press at <x> in a TextInput puts the cursor at <position>
      When the user presses the pointer at <x>, 110
      And the user releases the pointer at <x>, 110
      Then the property "cursorPosition" of "field" is <position>

      Examples:
        | x  | position |
        | 5  | 0        |
        | 15 | 1        |
