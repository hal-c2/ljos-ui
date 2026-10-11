# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TextInput
  One line of plain text the user can edit.

  Rule: A new TextInput is empty and editable

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput { objectName: "input" }
        }
        """

    Scenario Outline: An empty TextInput has its defaults
      Then the property "<property>" of "input" is <value>

      Examples:
        | property                     | value     |
        | text                         | ""        |
        | displayText                  | ""        |
        | length                       | 0         |
        | cursorPosition               | 0         |
        | selectionStart               | 0         |
        | selectionEnd                 | 0         |
        | selectedText                 | ""        |
        | color                        | "#000000" |
        | selectionColor               | "#000080" |
        | selectedTextColor            | "#ffffff" |
        | readOnly                     | false     |
        | echoMode                     | 0         |
        | passwordCharacter            | "•"       |
        | maximumLength                | 32767     |
        | acceptableInput              | true      |
        | autoScroll                   | true      |
        | selectByMouse                | true      |
        | mouseSelectionMode           | 0         |
        | persistentSelection          | false     |
        | inputMethodHints             | 0         |
        | inputMethodComposing         | false     |
        | preeditText                  | ""        |
        | overwriteMode                | false     |
        | activeFocusOnPress           | true      |
        | activeFocusOnTab             | false     |
        | cursorVisible                | false     |
        | canUndo                      | false     |
        | canRedo                      | false     |
        | horizontalAlignment          | 1         |
        | effectiveHorizontalAlignment | 1         |
        | verticalAlignment            | 32        |
        | wrapMode                     | 0         |
        | contentWidth                 | 0         |
        | implicitWidth                | 0         |
        | padding                      | 0         |
        | leftPadding                  | 0         |
        | bottomPadding                | 0         |
        | focus                        | false     |
        | clip                         | false     |
        | validator                    | null      |
        | cursorDelegate               | null      |

    Scenario: An empty TextInput is one line high
      Then the property "contentHeight" of "input" is at least 10
      And the property "cursorRectangle.width" of "input" is 1
      And the property "cursorRectangle.height" of "input" is at least 10

  Rule: The text can be given and changed from the application

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
            property int changes: 0
            onTextEdited: edits++
            onTextChanged: changes++
          }
        }
        """

    Scenario Outline: A TextInput created with text has its cursor at the end
      Then the property "<property>" of "input" is <value>

      Examples:
        | property       | value               |
        | text           | "hello brave world" |
        | displayText    | "hello brave world" |
        | length         | 17                  |
        | cursorPosition | 17                  |
        | selectionStart | 17                  |
        | selectionEnd   | 17                  |
        | canUndo        | false               |

    Scenario: Assigning the text moves the cursor to its end
      When the property "text" of "input" is set to "hi"
      Then the property "cursorPosition" of "input" is 2
      And the property "length" of "input" is 2
      And the property "changes" of "input" is 1

    Scenario: Assigning the text is not an edit by the user
      When the property "text" of "input" is set to "hi"
      Then the property "edits" of "input" is 0
      And the property "canUndo" of "input" is false

    Scenario: Assigning the same text changes nothing
      When the property "text" of "input" is set to "hello brave world"
      Then the property "changes" of "input" is 0

    Scenario: The cursor can be put anywhere in the text
      When the property "cursorPosition" of "input" is set to 3
      Then the property "cursorPosition" of "input" is 3

    Scenario Outline: A cursor position outside the text is ignored
      When the property "cursorPosition" of "input" is set to <position>
      Then the property "cursorPosition" of "input" is 17

      Examples:
        | position |
        | 100      |
        | -1       |

    Scenario: clear empties the text and can be undone
      When the method "clear" of "input" is called
      Then the property "text" of "input" is ""
      And the property "canUndo" of "input" is true
      And the property "edits" of "input" is 0

    Scenario: The line is as wide as its text, rounded up
      Then the property "implicitWidth" of "input" is at least 50
      And the property "width" of "input" is at least 50

  Rule: maximumLength limits the text

    Scenario: A text longer than maximumLength is cut
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput { objectName: "input"; maximumLength: 5; text: "abcdefghij" }
        }
        """
      Then the property "text" of "input" is "abcde"

    Scenario: An insertion is cut at maximumLength
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput { objectName: "input"; maximumLength: 5; text: "ab" }
        }
        """
      When the method "insert" of "input" is called with 2, "12345"
      Then the property "text" of "input" is "ab123"

    Scenario: Typing stops at maximumLength
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput { objectName: "input"; focus: true; maximumLength: 3 }
        }
        """
      When the user types "abcdef"
      Then the property "text" of "input" is "abc"

  Rule: A read-only TextInput cannot be edited by the user

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextInput {
            objectName: "input"; focus: true
            readOnly: true; text: "hello"
          }
        }
        """

    Scenario: Typing changes nothing
      When the user types "abc"
      Then the property "text" of "input" is "hello"

    Scenario: Backspace changes nothing
      When the user presses "Backspace"
      Then the property "text" of "input" is "hello"

    Scenario: The cursor still moves and selects
      When the user presses "Shift+Left"
      Then the property "selectedText" of "input" is "o"
      And the property "cursorPosition" of "input" is 4

    Scenario: The application can still change the text
      When the method "insert" of "input" is called with 0, "oh "
      Then the property "text" of "input" is "oh hello"
      And the property "canUndo" of "input" is false

    Scenario: A read-only TextInput shows no cursor
      Then the property "activeFocus" of "input" is true
      And the property "cursorVisible" of "input" is false
