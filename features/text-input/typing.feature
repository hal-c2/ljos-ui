# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#textEdited-signal
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#accepted-signal
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#editingFinished-signal
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#overwriteMode-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Typing into a TextInput
  The TextInput with active focus takes what the user types.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 200
        property string log: ""
        property string keys: ""
        Keys.onPressed: (e) => { if (e.key < 0x1000020 || e.key > 0x1000023) keys += e.key.toString(16) + " " }
        TextInput {
          objectName: "input"; focus: true
          width: 200
          onTextEdited: root.log += "edited "
          onAccepted: root.log += "accepted "
          onEditingFinished: root.log += "finished "
        }
        Item { objectName: "other" }
      }
      """

  Rule: Characters go in at the cursor

    Scenario: Typed text appears and the cursor follows it
      When the user types "Hello, World"
      Then the property "text" of "input" is "Hello, World"
      And the property "cursorPosition" of "input" is 12
      And the property "length" of "input" is 12

    Scenario: Each typed character is an edit
      When the user types "abc"
      Then the property "log" of "root" is "edited edited edited "
      And the property "keys" of "root" is ""

    Scenario: The focused TextInput shows its cursor
      Then the property "cursorVisible" of "input" is true

    Scenario: Typing in the middle inserts
      When the user types "held"
      And the user presses "Left"
      And the user types "l"
      Then the property "text" of "input" is "helld"
      And the property "cursorPosition" of "input" is 4

    Scenario: Typing replaces the selection
      When the user types "hello world"
      And the method "select" of "input" is called with 0, 5
      And the user types "bye"
      Then the property "text" of "input" is "bye world"
      And the property "cursorPosition" of "input" is 3

    Scenario: In overwrite mode a typed character replaces the one after the cursor
      When the user types "hello"
      And the property "overwriteMode" of "input" is set to true
      And the user presses "Home"
      And the user types "ab"
      Then the property "text" of "input" is "abllo"

  Rule: Backspace and Delete remove text

    Scenario Outline: <key> at position <from> of "hello brave world"
      When the user types "hello brave world"
      And the property "cursorPosition" of "input" is set to <from>
      And the user presses "<key>"
      Then the property "text" of "input" is <text>
      And the property "cursorPosition" of "input" is <cursor>

      Examples:
        | key             | from | text                | cursor |
        | Backspace       | 17   | "hello brave worl"  | 16     |
        | Backspace       | 0    | "hello brave world" | 0      |
        | Shift+Backspace | 17   | "hello brave worl"  | 16     |
        | Delete          | 0    | "ello brave world"  | 0      |
        | Delete          | 17   | "hello brave world" | 17     |
        | Ctrl+D          | 0    | "ello brave world"  | 0      |
        | Ctrl+Backspace  | 17   | "hello brave "      | 12     |
        | Ctrl+Backspace  | 8    | "hello ave world"   | 6      |
        | Ctrl+Delete     | 0    | "brave world"       | 0      |
        | Ctrl+K          | 5    | "hello"             | 5      |
        | Ctrl+U          | 5    | ""                  | 0      |

    Scenario: Backspace removes the selection
      When the user types "hello brave world"
      And the method "select" of "input" is called with 5, 11
      And the user presses "Backspace"
      Then the property "text" of "input" is "hello world"
      And the property "cursorPosition" of "input" is 5

  Rule: Return accepts the input

    Scenario: Return emits editingFinished and then accepted
      When the user types "a"
      And the user presses "Return"
      Then the property "log" of "root" is "edited finished accepted "
      And the property "text" of "input" is "a"

    Scenario: Enter on the keypad does the same
      When the user presses "Enter"
      Then the property "log" of "root" is "finished accepted "

    Scenario: The press goes on to the parents
      When the user presses "Return"
      Then the property "keys" of "root" is "1000004 "

  Rule: Losing focus ends the editing

    Scenario: editingFinished is emitted when another item takes focus
      When the user types "a"
      And the method "forceActiveFocus" of "other" is called
      Then the property "log" of "root" is "edited finished "
      And the property "cursorVisible" of "input" is false

    Scenario: The selection is dropped with the focus
      When the user types "abc"
      And the method "selectAll" of "input" is called
      And the method "forceActiveFocus" of "other" is called
      Then the property "selectedText" of "input" is ""

    Scenario: A persistent selection stays
      When the user types "abc"
      And the property "persistentSelection" of "input" is set to true
      And the method "selectAll" of "input" is called
      And the method "forceActiveFocus" of "other" is called
      Then the property "selectedText" of "input" is "abc"
