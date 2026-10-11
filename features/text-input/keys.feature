# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Moving and selecting in a TextInput with the keyboard
  The platform's standard keys move the cursor by character, word and line,
  and select when Shift is held. Keys the TextInput has no use for go on to
  its parents.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property string keys: ""
        Keys.onPressed: (e) => { if (e.key < 0x1000020 || e.key > 0x1000023) keys += e.key.toString(16) + " " }
        TextInput {
          objectName: "input"; focus: true
          width: 200
          text: "hello, brave-world foo"
        }
      }
      """

  Rule: The cursor moves by character, word and line

    Scenario Outline: <key> from position <from>
      When the property "cursorPosition" of "input" is set to <from>
      And the user presses "<key>"
      Then the property "cursorPosition" of "input" is <to>
      And the property "selectedText" of "input" is ""

      Examples:
        | key        | from | to |
        | Left       | 5    | 4  |
        | Right      | 5    | 6  |
        | Home       | 5    | 0  |
        | End        | 5    | 22 |
        | Ctrl+E     | 5    | 22 |
        | Ctrl+Right | 0    | 5  |
        | Ctrl+Right | 5    | 7  |
        | Ctrl+Right | 7    | 12 |
        | Ctrl+Right | 12   | 13 |
        | Ctrl+Right | 13   | 19 |
        | Ctrl+Right | 19   | 22 |
        | Ctrl+Left  | 22   | 19 |
        | Ctrl+Left  | 19   | 13 |
        | Ctrl+Left  | 13   | 12 |
        | Ctrl+Left  | 12   | 7  |
        | Ctrl+Left  | 7    | 5  |
        | Ctrl+Left  | 5    | 0  |
        | Ctrl+Left  | 9    | 7  |

  Rule: Shift selects

    Scenario Outline: <key> from position 7
      When the property "cursorPosition" of "input" is set to 7
      And the user presses "<key>"
      Then the property "cursorPosition" of "input" is <cursor>
      And the property "selectedText" of "input" is <selected>

      Examples:
        | key              | cursor | selected          |
        | Shift+Left       | 6      | " "               |
        | Shift+Right      | 8      | "b"               |
        | Shift+Home       | 0      | "hello, "         |
        | Shift+End        | 22     | "brave-world foo" |
        | Ctrl+Shift+Right | 12     | "brave"           |
        | Ctrl+Shift+Left  | 5      | ", "              |

    Scenario: Select All selects everything and puts the cursor at the end
      When the property "cursorPosition" of "input" is set to 7
      And the user presses "Ctrl+A"
      Then the property "selectedText" of "input" is "hello, brave-world foo"
      And the property "cursorPosition" of "input" is 22

    Scenario Outline: <key> leaves a selection at its <side>
      When the method "select" of "input" is called with 7, 12
      And the user presses "<key>"
      Then the property "cursorPosition" of "input" is <cursor>
      And the property "selectedText" of "input" is ""

      Examples:
        | key   | side  | cursor |
        | Left  | start | 7      |
        | Right | end   | 12     |

  Rule: A key the TextInput does not use goes on to its parents

    Scenario Outline: <key> is passed on
      When the property "cursorPosition" of "input" is set to <from>
      And the user presses "<key>"
      Then the property "keys" of "root" is "<seen> "
      And the property "cursorPosition" of "input" is <from>

      Examples:
        | key    | from | seen    |
        | Up     | 5    | 1000013 |
        | Down   | 5    | 1000015 |
        | Escape | 5    | 1000000 |
        | Tab    | 5    | 1000001 |
        | F5     | 5    | 1000034 |
        | Ctrl+S | 5    | 53      |
        | Left   | 0    | 1000012 |
        | Right  | 22   | 1000014 |

    Scenario Outline: <key> is kept even when it changes nothing
      When the property "cursorPosition" of "input" is set to <from>
      And the user presses "<key>"
      Then the property "keys" of "root" is ""

      Examples:
        | key       | from |
        | Backspace | 0    |
        | Delete    | 22   |
        | Home      | 0    |
        | End       | 22   |
        | Ctrl+Z    | 5    |
        | Ctrl+V    | 5    |
        | Ctrl+C    | 5    |
        | Left      | 5    |
        | A         | 5    |
