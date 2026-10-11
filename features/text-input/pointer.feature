# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#selectByMouse-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#mouseSelectionMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#activeFocusOnPress-prop
# Verified on Qt 6.11.2 (qml6, offscreen; pointer input over its vnc platform).

@qt-parity
Feature: Using a TextInput with the pointer
  A press puts the cursor under the pointer, a double click selects a word
  and a drag selects from where it began to where it is. The numbers are
  those of the bundled font, Liberation Sans, at 14 pixels.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "input"
          font.family: "Liberation Sans"; font.pixelSize: 14
          width: 200; height: 60
          text: "hello brave world"
        }
      }
      """

  Rule: A press places the cursor and takes the focus

    Scenario Outline: A press at <x> puts the cursor at <position>
      When the user touches <x>, 5
      Then the property "cursorPosition" of "input" is <position>
      And the property "activeFocus" of "input" is true
      And the property "cursorVisible" of "input" is true

      Examples:
        | x   | position |
        | 2   | 0        |
        | 5   | 1        |
        | 31  | 5        |
        | 190 | 17       |

    Scenario: A press drops the selection
      When the method "selectAll" of "input" is called
      And the user touches 31, 5
      Then the property "selectedText" of "input" is ""
      And the property "cursorPosition" of "input" is 5

    Scenario: Without activeFocusOnPress a press leaves the focus alone
      When the property "activeFocusOnPress" of "input" is set to false
      And the user touches 31, 5
      Then the property "activeFocus" of "input" is false

    Scenario: A disabled TextInput ignores the pointer
      When the property "enabled" of "input" is set to false
      And the user touches 31, 5
      Then the property "cursorPosition" of "input" is 17
      And the property "activeFocus" of "input" is false

    Scenario: A read-only TextInput still takes the focus and the cursor
      When the property "readOnly" of "input" is set to true
      And the user touches 31, 5
      Then the property "cursorPosition" of "input" is 5
      And the property "activeFocus" of "input" is true

  Rule: A double click selects a word

    Scenario Outline: A double click at <x> selects <selected>
      When the user double-clicks at <x>, 5
      Then the property "selectedText" of "input" is <selected>
      And the property "cursorPosition" of "input" is <cursor>

      Examples:
        | x   | selected | cursor |
        | 50  | "brave"  | 11     |
        | 10  | "hello"  | 5      |
        | 190 | "world"  | 17     |

    Scenario: Typing after a double click replaces the word
      When the user double-clicks at 50, 5
      And the user types "old"
      Then the property "text" of "input" is "hello old world"

  Rule: A drag selects

    Scenario: A drag selects from where it began to where it ended
      When the user drags from 31, 5 to 70, 5
      Then the property "selectedText" of "input" is " brave"
      And the property "cursorPosition" of "input" is 11

    Scenario: A drag backwards leaves the cursor at the start
      When the user drags from 190, 5 to 2, 5
      Then the property "selectedText" of "input" is "hello brave world"
      And the property "cursorPosition" of "input" is 0

    Scenario: A drag goes on outside the item
      When the user drags from 31, 5 to 250, 150
      Then the property "selectedText" of "input" is " brave world"

    Scenario: In the word selection mode a drag selects whole words
      When the property "mouseSelectionMode" of "input" is set to 1
      And the user drags from 10, 5 to 50, 5
      Then the property "selectedText" of "input" is "hello brave"

    Scenario: Without selectByMouse a drag selects nothing
      When the property "selectByMouse" of "input" is set to false
      And the user drags from 31, 5 to 70, 5
      Then the property "selectedText" of "input" is ""

    Scenario: Without selectByMouse a double click selects nothing
      When the property "selectByMouse" of "input" is set to false
      And the user double-clicks at 50, 5
      Then the property "selectedText" of "input" is ""
