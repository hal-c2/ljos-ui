# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TextEdit
  Several lines of plain text the user can edit. The numbers are those of
  the bundled font, Liberation Sans, at 14 pixels.

  Rule: A new TextEdit is empty and editable

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextEdit { objectName: "edit" }
        }
        """

    Scenario Outline: An empty TextEdit has its defaults
      Then the property "<property>" of "edit" is <value>

      Examples:
        | property            | value     |
        | text                | ""        |
        | length              | 0         |
        | lineCount           | 1         |
        | cursorPosition      | 0         |
        | selectionStart      | 0         |
        | selectionEnd        | 0         |
        | selectedText        | ""        |
        | color               | "#000000" |
        | selectionColor      | "#000080" |
        | selectedTextColor   | "#ffffff" |
        | readOnly            | false     |
        | selectByMouse       | true      |
        | selectByKeyboard    | true      |
        | mouseSelectionMode  | 0         |
        | persistentSelection | false     |
        | textFormat          | 0         |
        | textMargin          | 0         |
        | tabStopDistance     | 80        |
        | wrapMode            | 0         |
        | horizontalAlignment | 1         |
        | verticalAlignment   | 32        |
        | inputMethodHints    | 0         |
        | overwriteMode       | false     |
        | activeFocusOnPress  | true      |
        | cursorVisible       | false     |
        | canUndo             | false     |
        | canRedo             | false     |
        | contentWidth        | 0         |
        | paintedWidth        | 0         |
        | implicitWidth       | 0         |
        | padding             | 0         |
        | cursorDelegate      | null      |

  Rule: The text has lines

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextEdit {
            objectName: "edit"
            font.family: "Liberation Sans"; font.pixelSize: 14
            text: "hello brave world\nsecond line"
            property real probe: 0
          }
        }
        """

    Scenario Outline: A text of two lines
      Then the property "<property>" of "edit" is <value>

      Examples:
        | property               | value  |
        | lineCount              | 2      |
        | length                 | 29     |
        | cursorPosition         | 0      |
        | contentWidth           | 105.75 |
        | contentHeight          | 32     |
        | paintedWidth           | 105.75 |
        | paintedHeight          | 32     |
        | implicitWidth          | 105.75 |
        | implicitHeight         | 32     |
        | cursorRectangle.x      | 0      |
        | cursorRectangle.y      | 0      |
        | cursorRectangle.width  | 1      |
        | cursorRectangle.height | 16     |

    @api-QtQuick.TextEdit.paintedWidth @api-QtQuick.TextEdit.paintedHeight
    Scenario: The painted size follows the text
      When the property "text" of "edit" is set to "hello brave world"
      Then the property "paintedWidth" of "edit" is 105.75
      And the property "paintedHeight" of "edit" is 16

    Scenario Outline: The cursor at position <position> is at <x>, <y>
      When the property "cursorPosition" of "edit" is set to <position>
      Then the property "cursorRectangle.x" of "edit" is <x>
      And the property "cursorRectangle.y" of "edit" is <y>

      Examples:
        | position | x        | y  |
        | 17       | 105.75   | 0  |
        | 18       | 0        | 16 |
        | 20       | 14.78125 | 16 |

    Scenario Outline: positionAt <x>, <y> is <position>
      When the property "probe" of "edit" is bound to "positionAt(<x>, <y>)"
      Then the property "probe" of "edit" is <position>

      Examples:
        | x   | y  | position |
        | 5   | 20 | 19       |
        | 500 | 5  | 17       |
        | 50  | 31 | 25       |
        | 50  | 32 | 29       |
        | 50  | 15 | 9        |
        | 50  | -1 | 0        |

    Scenario: A line break at the end starts an empty line
      When the property "text" of "edit" is set to "a\n"
      Then the property "lineCount" of "edit" is 2
      And the property "contentHeight" of "edit" is 32

    Scenario: Assigning the text puts the cursor at the start
      When the property "cursorPosition" of "edit" is set to 5
      And the property "text" of "edit" is set to "other"
      Then the property "cursorPosition" of "edit" is 0

    Scenario: A right-aligned text is aligned line by line
      When the property "width" of "edit" is set to 200
      And the property "horizontalAlignment" of "edit" is set to 2
      And the property "cursorPosition" of "edit" is set to 29
      Then the property "cursorRectangle.x" of "edit" is 200
      When the property "cursorPosition" of "edit" is set to 17
      Then the property "cursorRectangle.x" of "edit" is 200

    Scenario: Padding moves the text and adds to the implicit size
      When the property "padding" of "edit" is set to 8
      Then the property "cursorRectangle.x" of "edit" is 8
      And the property "cursorRectangle.y" of "edit" is 8
      And the property "implicitWidth" of "edit" is 121.75
      And the property "implicitHeight" of "edit" is 48

  Rule: Long lines can wrap

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextEdit {
            objectName: "edit"; focus: true
            font.family: "Liberation Sans"; font.pixelSize: 14
            width: 60
            wrapMode: TextEdit.Wrap
            text: "hello brave world again"
          }
        }
        """

    Scenario Outline: A wrapped text
      Then the property "<property>" of "edit" is <value>

      Examples:
        | property       | value      |
        | lineCount      | 4          |
        | contentHeight  | 64         |
        | contentWidth   | 35         |
        | implicitWidth  | 143.859375 |
        | implicitHeight | 64         |

    Scenario Outline: The cursor at position <position> of a wrapped text
      When the property "cursorPosition" of "edit" is set to <position>
      Then the property "cursorRectangle.x" of "edit" is <x>
      And the property "cursorRectangle.y" of "edit" is <y>

      Examples:
        | position | x       | y  |
        | 5        | 29.5625 | 0  |
        | 6        | 0       | 16 |

    Scenario: End goes to the end of the wrapped line, before its space
      When the user presses "End"
      Then the property "cursorPosition" of "edit" is 5

    Scenario: Home goes to the start of the wrapped line
      When the property "cursorPosition" of "edit" is set to 9
      And the user presses "Home"
      Then the property "cursorPosition" of "edit" is 6

    Scenario: Down keeps the cursor's place in the line
      When the property "cursorPosition" of "edit" is set to 1
      And the user presses "Down"
      Then the property "cursorPosition" of "edit" is 7

    Scenario: A wider TextEdit needs fewer lines
      When the property "width" of "edit" is set to 120
      Then the property "lineCount" of "edit" is 2

  Rule: The text can be changed from the application

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          width: 300; height: 200
          TextEdit {
            objectName: "edit"
            text: "hello brave world\nsecond line"
          }
        }
        """

    Scenario: select puts the cursor at the end it is given last
      When the method "select" of "edit" is called with 2, 20
      Then the property "cursorPosition" of "edit" is 20
      And the property "selectedText" of "edit" is "llo brave world\nse"

    Scenario: selectWord selects the word at the cursor
      When the property "cursorPosition" of "edit" is set to 20
      And the method "selectWord" of "edit" is called
      Then the property "selectionStart" of "edit" is 18
      And the property "selectionEnd" of "edit" is 24
      And the property "cursorPosition" of "edit" is 24

    Scenario: append adds a new paragraph at the end
      When the method "append" of "edit" is called with "tail"
      Then the property "text" of "edit" is "hello brave world\nsecond line\ntail"
      And the property "lineCount" of "edit" is 3

    Scenario: Insertions that follow one another are one step to undo
      When the method "insert" of "edit" is called with 0, "a"
      And the method "insert" of "edit" is called with 1, "b"
      And the method "undo" of "edit" is called
      Then the property "text" of "edit" is "hello brave world\nsecond line"
      And the property "canUndo" of "edit" is false

    Scenario: An insertion somewhere else is a step of its own
      When the method "insert" of "edit" is called with 0, "a"
      And the method "insert" of "edit" is called with 5, "b"
      And the method "undo" of "edit" is called
      Then the property "text" of "edit" is "ahello brave world\nsecond line"

    Scenario: Each removal is a step of its own
      When the method "remove" of "edit" is called with 3, 5
      And the method "remove" of "edit" is called with 3, 5
      And the method "undo" of "edit" is called
      Then the property "text" of "edit" is "hel brave world\nsecond line"

    Scenario Outline: A position outside the text is ignored
      When the property "cursorPosition" of "edit" is set to <position>
      Then the property "cursorPosition" of "edit" is 0

      Examples:
        | position |
        | 100      |
        | -1       |

    Scenario: remove stops at the end of the text
      When the method "remove" of "edit" is called with 5, 100
      Then the property "text" of "edit" is "hello"

    Scenario: clear can be undone
      When the method "clear" of "edit" is called
      Then the property "text" of "edit" is ""
      And the property "canUndo" of "edit" is true

    Scenario: getText and getFormattedText return plain text
      When the property "objectName" of "edit" is bound to "getText(18, 24) + '+' + getFormattedText(0, 5)"
      Then the object "second+hello" exists
