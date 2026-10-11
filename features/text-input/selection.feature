# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#select-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#selectWord-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#moveCursorSelection-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#insert-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#remove-method
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Selecting and changing the text of a TextInput from the application
  The methods a script uses: select, selectAll, selectWord, deselect,
  moveCursorSelection, insert, remove and getText.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "input"
          text: "hello brave world"
          property int edits: 0
          onTextEdited: edits++
        }
      }
      """

  Rule: select puts the cursor at the end it is given last

    Scenario Outline: select from <from> to <to>
      When the method "select" of "input" is called with <from>, <to>
      Then the property "cursorPosition" of "input" is <cursor>
      And the property "selectionStart" of "input" is <start>
      And the property "selectionEnd" of "input" is <end>
      And the property "selectedText" of "input" is <selected>

      Examples:
        | from | to  | cursor | start | end | selected |
        | 2    | 7   | 7      | 2     | 7   | "llo b"  |
        | 7    | 2   | 2      | 2     | 7   | "llo b"  |
        | 3    | 3   | 3      | 3     | 3   | ""       |
        | -1   | 5   | 17     | 17    | 17  | ""       |
        | 3    | 100 | 17     | 17    | 17  | ""       |

    Scenario: selectAll selects everything and puts the cursor at the end
      When the property "cursorPosition" of "input" is set to 3
      And the method "selectAll" of "input" is called
      Then the property "selectedText" of "input" is "hello brave world"
      And the property "cursorPosition" of "input" is 17

    Scenario: deselect keeps the cursor where it is
      When the method "select" of "input" is called with 7, 2
      And the method "deselect" of "input" is called
      Then the property "selectedText" of "input" is ""
      And the property "cursorPosition" of "input" is 2

    Scenario: Moving the cursor drops the selection
      When the method "select" of "input" is called with 2, 7
      And the property "cursorPosition" of "input" is set to 4
      Then the property "selectionStart" of "input" is 4
      And the property "selectionEnd" of "input" is 4

  Rule: selectWord selects the word at the cursor

    Scenario Outline: The word at position <position>
      When the property "cursorPosition" of "input" is set to <position>
      And the method "selectWord" of "input" is called
      Then the property "selectionStart" of "input" is <start>
      And the property "selectionEnd" of "input" is <end>
      And the property "cursorPosition" of "input" is <end>

      Examples:
        | position | start | end |
        | 0        | 0     | 5   |
        | 5        | 0     | 5   |
        | 6        | 6     | 11  |
        | 8        | 6     | 11  |
        | 17       | 12    | 17  |

  Rule: moveCursorSelection moves the cursor and keeps the other end

    Scenario: By characters
      When the property "cursorPosition" of "input" is set to 0
      And the method "moveCursorSelection" of "input" is called with 5
      Then the property "selectedText" of "input" is "hello"
      And the property "cursorPosition" of "input" is 5

    Scenario: By words, the selection grows to whole words
      When the property "cursorPosition" of "input" is set to 0
      And the method "moveCursorSelection" of "input" is called with 8, 1
      Then the property "selectedText" of "input" is "hello brave"
      And the property "cursorPosition" of "input" is 11

    Scenario: Backwards
      When the property "cursorPosition" of "input" is set to 9
      And the method "moveCursorSelection" of "input" is called with 2
      Then the property "selectionStart" of "input" is 2
      And the property "selectionEnd" of "input" is 9
      And the property "cursorPosition" of "input" is 2

  Rule: insert and remove change the text as an edit

    Scenario: insert puts text at a position
      When the method "insert" of "input" is called with 6, "very "
      Then the property "text" of "input" is "hello very brave world"
      And the property "edits" of "input" is 1
      And the property "canUndo" of "input" is true

    Scenario Outline: insert outside the text is ignored
      When the method "insert" of "input" is called with <position>, "x"
      Then the property "text" of "input" is "hello brave world"

      Examples:
        | position |
        | -1       |
        | 18       |

    Scenario Outline: remove takes its two ends in either order
      When the method "remove" of "input" is called with <from>, <to>
      Then the property "text" of "input" is "hello world"

      Examples:
        | from | to |
        | 5    | 11 |
        | 11   | 5  |

    Scenario Outline: getText returns a part of the text
      When the property "text" of "input" is bound to "getText(<from>, <to>)"
      Then the property "text" of "input" is <part>

      Examples:
        | from | to  | part    |
        | 6    | 11  | "brave" |
        | 11   | 6   | "brave" |
        | 12   | 100 | "world" |
