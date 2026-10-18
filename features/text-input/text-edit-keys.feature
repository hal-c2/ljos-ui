# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#selectByKeyboard-prop
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Editing a TextEdit with the keyboard
  A TextEdit takes the keys a TextInput takes, and Return, Up and Down
  besides.

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
        TextEdit {
          objectName: "edit"; focus: true
          font.family: "Liberation Sans"; font.pixelSize: 14
          width: 200; height: 100
          text: "hello brave\nworld\nlast line here"
          property int finishes: 0
          onEditingFinished: finishes++
        }
        Item { objectName: "other" }
      }
      """

  Rule: Return starts a new line

    Scenario Outline: <key> breaks the line at the cursor
      When the property "cursorPosition" of "edit" is set to 5
      And the user presses "<key>"
      Then the property "text" of "edit" is "hello\n brave\nworld\nlast line here"
      And the property "cursorPosition" of "edit" is 6
      And the property "lineCount" of "edit" is 4
      And the property "keys" of "root" is ""

      Examples:
        | key          |
        | Return       |
        | Enter        |
        | Shift+Return |

    # Each platform's InsertLineSeparator and InsertParagraphSeparator
    # (qkeysequence.html#standard-shortcuts); the oracle has only Windows's
    # key bindings.
    @ljos-only
    Scenario Outline: <key> breaks the line under <platform>'s key bindings
      Given the application uses the key bindings of "<platform>"
      When the property "cursorPosition" of "edit" is set to 5
      And the user presses "<key>"
      Then the property "text" of "edit" is "hello\n brave\nworld\nlast line here"
      And the property "cursorPosition" of "edit" is 6
      And the property "keys" of "root" is ""

      Examples:
        | platform | key          |
        | Linux    | Shift+Return |
        | Linux    | Shift+Enter  |
        | Windows  | Shift+Return |
        | Windows  | Shift+Enter  |
        | macOS    | Return       |
        | macOS    | Shift+Return |
        | macOS    | Shift+Enter  |
        | macOS    | Meta+Return  |
        | macOS    | Meta+Enter   |

    Scenario: A new line is a step of its own to undo
      When the property "text" of "edit" is set to ""
      And the user types "abc"
      And the user presses "Return"
      And the user types "def"
      And the user presses "Ctrl+Z"
      Then the property "text" of "edit" is "abc\n"
      When the user presses "Ctrl+Z"
      Then the property "text" of "edit" is "abc"

  Rule: The cursor moves by line and through the whole text

    Scenario Outline: <key> from position <from>
      When the property "cursorPosition" of "edit" is set to <from>
      And the user presses "<key>"
      Then the property "cursorPosition" of "edit" is <to>

      Examples:
        | key        | from | to |
        | Down       | 1    | 13 |
        | Down       | 13   | 20 |
        | Up         | 20   | 13 |
        | Up         | 13   | 1  |
        | Home       | 14   | 12 |
        | End        | 14   | 17 |
        | Ctrl+Home  | 14   | 0  |
        | Ctrl+End   | 14   | 32 |
        | Left       | 12   | 11 |
        | Right      | 11   | 12 |
        | Ctrl+Right | 6    | 11 |
        | Ctrl+Right | 11   | 12 |
        | Ctrl+Left  | 12   | 11 |

    Scenario: Up and Down remember the column they started from
      When the property "cursorPosition" of "edit" is set to 32
      And the user presses "Up"
      Then the property "cursorPosition" of "edit" is 17
      When the user presses "Up"
      Then the property "cursorPosition" of "edit" is 11
      When the user presses "Down"
      And the user presses "Down"
      Then the property "cursorPosition" of "edit" is 32

    Scenario: Shift+Down selects to the same place in the next line
      When the property "cursorPosition" of "edit" is set to 1
      And the user presses "Shift+Down"
      Then the property "cursorPosition" of "edit" is 13
      And the property "selectedText" of "edit" is "ello brave\nw"

    # Ctrl+K is DeleteEndOfLine on Linux only (qml6 on xcb deletes, checked
    # with qmltestrunner); the oracle's qml6 has Windows's key bindings.
    Scenario: Ctrl+K deletes nothing under Windows's key bindings
      Given the application uses the key bindings of "Windows"
      When the property "cursorPosition" of "edit" is set to 1
      And the user presses "Ctrl+K"
      Then the property "text" of "edit" is "hello brave\nworld\nlast line here"
      And the property "cursorPosition" of "edit" is 1

    @ljos-only
    Scenario Outline: Ctrl+K deletes to the end of the line under <platform>'s key bindings
      Given the application uses the key bindings of "<platform>"
      When the property "cursorPosition" of "edit" is set to 1
      And the user presses "Ctrl+K"
      Then the property "text" of "edit" is "h\nworld\nlast line here"

      Examples:
        | platform   |
        | Linux      |
        | GNOME      |
        | KDE Plasma |

  Rule: A key the TextEdit does not use goes on to its parents

    Scenario Outline: <key> at position <from> is passed on
      When the property "cursorPosition" of "edit" is set to <from>
      And the user presses "<key>"
      Then the property "keys" of "root" is "<seen> "
      And the property "cursorPosition" of "edit" is <from>

      Examples:
        | key    | from | seen    |
        | Up     | 3    | 1000013 |
        | Down   | 25   | 1000015 |
        | PgUp   | 25   | 1000016 |
        | PgDown | 3    | 1000017 |
        | Escape | 3    | 1000000 |

  Rule: A read-only TextEdit leaves the keyboard to others

    Scenario Outline: <key> is passed on by a read-only TextEdit
      When the property "readOnly" of "edit" is set to true
      And the property "cursorPosition" of "edit" is set to 3
      And the user presses "<key>"
      Then the property "keys" of "root" is "<seen> "
      And the property "cursorPosition" of "edit" is 3
      And the property "text" of "edit" is "hello brave\nworld\nlast line here"

      Examples:
        | key       | seen    |
        | Right     | 1000014 |
        | Down      | 1000015 |
        | Home      | 1000010 |
        | Return    | 1000004 |
        | Backspace | 1000003 |
        | A         | 41      |

    Scenario: A read-only TextEdit is not selected by keyboard unless asked
      When the property "readOnly" of "edit" is set to true
      Then the property "selectByKeyboard" of "edit" is false

    Scenario: Select All still works in a read-only TextEdit
      When the property "readOnly" of "edit" is set to true
      And the user presses "Ctrl+A"
      Then the property "selectionStart" of "edit" is 0
      And the property "selectionEnd" of "edit" is 32

    Scenario: With selectByKeyboard a read-only TextEdit moves its cursor
      When the property "readOnly" of "edit" is set to true
      And the property "selectByKeyboard" of "edit" is set to true
      And the property "cursorPosition" of "edit" is set to 3
      And the user presses "Shift+Right"
      Then the property "selectedText" of "edit" is "l"
      And the property "keys" of "root" is ""

    Scenario: Without selectByKeyboard Shift+Right is passed on
      When the property "selectByKeyboard" of "edit" is set to false
      And the user presses "Shift+Right"
      Then the property "selectedText" of "edit" is ""
      And the property "keys" of "root" is "1000014 "

  Rule: Losing focus ends the editing

    Scenario: editingFinished is emitted when another item takes focus
      When the method "forceActiveFocus" of "other" is called
      Then the property "finishes" of "edit" is 1
      And the property "cursorVisible" of "edit" is false

  Rule: The pointer places the cursor in any line

    Scenario Outline: A press at <x>, <y> puts the cursor at <position>
      When the user touches <x>, <y>
      Then the property "cursorPosition" of "edit" is <position>

      Examples:
        | x   | y  | position |
        | 8   | 20 | 13       |
        | 190 | 5  | 11       |
        | 50  | 90 | 32       |

    Scenario: A double click selects a word in its line
      When the user double-clicks at 10, 20
      Then the property "selectedText" of "edit" is "world"

    Scenario: A drag selects across lines
      When the user drags from 31, 5 to 8, 20
      Then the property "selectedText" of "edit" is " brave\nw"
