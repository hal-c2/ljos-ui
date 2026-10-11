# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#preeditText-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#inputMethodComposing-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#inputMethodHints-prop
#   https://doc.qt.io/qt-6/qml-qtquick-textedit.html#preeditText-prop

# The oracle's input platform has no input method, so nothing here can be
# observed on Qt.
@ljos-only
Feature: Composing text with an input method
  An input method shows what is being composed at the cursor, underlined,
  before it becomes part of the text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "input"; focus: true
          font.family: "Liberation Sans"; font.pixelSize: 14
          width: 200
          text: "ab"
          property int edits: 0
          property int changes: 0
          onTextEdited: edits++
          onTextChanged: changes++
          property IntValidator numbers: IntValidator {}
        }
        TextEdit { objectName: "edit"; y: 50; width: 200; text: "one\ntwo" }
      }
      """

  Rule: What is being composed is shown but is not yet text

    Scenario: The composition is not part of the text
      When the input method composes "ni"
      Then the property "preeditText" of "input" is "ni"
      And the property "inputMethodComposing" of "input" is true
      And the property "text" of "input" is "ab"
      And the property "changes" of "input" is 0

    Scenario: The composition is displayed at the cursor
      When the input method composes "ni"
      Then the property "displayText" of "input" is "abni"
      And the property "contentWidth" of "input" is at least 25
      And the property "cursorPosition" of "input" is 2

    Scenario: The cursor is drawn at the end of the composition
      When the input method composes "ni"
      Then the property "cursorRectangle.x" of "input" is at least 25

    Scenario: A composition replaces the one before it
      When the input method composes "n"
      And the input method composes "ni"
      And the input method composes "nih"
      Then the property "preeditText" of "input" is "nih"
      And the property "text" of "input" is "ab"

  Rule: Committing turns the composition into text

    Scenario: The committed text is typed at the cursor
      When the input method composes "ni"
      And the input method commits "你"
      Then the property "text" of "input" is "ab你"
      And the property "preeditText" of "input" is ""
      And the property "inputMethodComposing" of "input" is false
      And the property "cursorPosition" of "input" is 3
      And the property "edits" of "input" is 1

    Scenario: A commit without a composition types the text
      When the input method commits "é"
      Then the property "text" of "input" is "abé"

    Scenario: A commit replaces the selection
      When the method "selectAll" of "input" is called
      And the input method commits "你好"
      Then the property "text" of "input" is "你好"

    Scenario: A committed text is one step to undo
      When the input method composes "ni"
      And the input method commits "你"
      And the user presses "Ctrl+Z"
      Then the property "text" of "input" is "ab"

    Scenario: Composing nothing ends the composition without typing
      When the input method composes "ni"
      And the input method composes ""
      Then the property "inputMethodComposing" of "input" is false
      And the property "text" of "input" is "ab"
      And the property "displayText" of "input" is "ab"

  Rule: A composition does not outlive its place

    Scenario: Losing focus ends the composition without typing it
      When the input method composes "ni"
      And the method "forceActiveFocus" of "edit" is called
      Then the property "preeditText" of "input" is ""
      And the property "text" of "input" is "ab"

    Scenario: A read-only TextInput takes no composition
      When the property "readOnly" of "input" is set to true
      And the input method composes "ni"
      And the input method commits "你"
      Then the property "preeditText" of "input" is ""
      And the property "text" of "input" is "ab"

    Scenario: A validator judges the committed text
      When the property "text" of "input" is set to ""
      And the property "validator" of "input" is bound to "numbers"
      And the input method commits "x"
      Then the property "text" of "input" is ""

    Scenario: maximumLength cuts a committed text
      When the property "maximumLength" of "input" is set to 3
      And the input method commits "你好"
      Then the property "text" of "input" is "ab你"

  Rule: A TextEdit composes the same way

    Scenario: A composition in a TextEdit
      When the method "forceActiveFocus" of "edit" is called
      And the input method composes "ni"
      Then the property "preeditText" of "edit" is "ni"
      And the property "inputMethodComposing" of "edit" is true
      And the property "text" of "edit" is "one\ntwo"

    Scenario: A commit in a TextEdit
      When the method "forceActiveFocus" of "edit" is called
      And the property "cursorPosition" of "edit" is set to 3
      And the input method commits "!"
      Then the property "text" of "edit" is "one!\ntwo"

  Rule: Without a text item the input method has nowhere to type

    Scenario: A commit with no editor in focus is dropped
      When the method "forceActiveFocus" of "input" is called
      And the property "focus" of "input" is set to false
      And the input method commits "x"
      Then the property "text" of "input" is "ab"
      And the property "text" of "edit" is "one\ntwo"
