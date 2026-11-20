# Sources:
#   https://developer.android.com/games/agdk/add-support-for-text-input
#   https://developer.android.com/reference/android/view/inputmethod/InputConnection#setComposingRegion(int,%20int)
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#preeditText-prop
#   https://github.com/hal-c2/ljos-ui/issues/615
# Some input methods tell the whole of what they hold after every change
# rather than what changed: the text, the selection and the part still
# being composed (Android's GameTextInput). The editor takes what changed
# from it. Positions count UTF-16 code units, as the platform's do. The
# oracle's input platform has no input method, so these are Ljós's own.
@ljos-only
Feature: An input method that tells the whole of what it holds
  The editor with the active focus becomes what the input method holds:
  what is composed is shown but is not yet text, a commit types it, and a
  selection the method makes is the editor's.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        TextInput {
          objectName: "input"; focus: true
          font.family: "Liberation Sans"; font.pixelSize: 14
          width: 200
          text: "ab"
          property int edits: 0
          onTextEdited: edits++
        }
        TextInput {
          objectName: "pin"; y: 50; width: 200
          echoMode: TextInput.Password
          text: "12"
        }
      }
      """

  Rule: What changed is typed into the editor

    Scenario: A character typed at the end
      When the input method holds "abc" with the cursor at 3
      Then the property "text" of "input" is "abc"
      And the property "cursorPosition" of "input" is 3
      And the property "edits" of "input" is 1

    Scenario: A character deleted
      When the input method holds "a" with the cursor at 1
      Then the property "text" of "input" is "a"
      And the property "cursorPosition" of "input" is 1

    Scenario: A character typed in the middle
      When the input method holds "aXb" with the cursor at 2
      Then the property "text" of "input" is "aXb"
      And the property "cursorPosition" of "input" is 2

    Scenario: A character outside the Basic Multilingual Plane counts two
      When the input method holds "ab𝄞" with the cursor at 4
      Then the property "text" of "input" is "ab𝄞"
      And the property "cursorPosition" of "input" is 4

  Rule: The composing region is the editor's composition

    Scenario: What is composed is not yet text
      When the input method holds "abni" composing 2 to 4 with the cursor at 4
      Then the property "text" of "input" is "ab"
      And the property "preeditText" of "input" is "ni"
      And the property "inputMethodComposing" of "input" is true
      And the property "edits" of "input" is 0

    Scenario: A composition that ends types what it composed
      When the input method holds "abni" composing 2 to 4 with the cursor at 4
      And the input method holds "ab日本" with the cursor at 4
      Then the property "text" of "input" is "ab日本"
      And the property "preeditText" of "input" is ""
      And the property "inputMethodComposing" of "input" is false
      And the property "cursorPosition" of "input" is 4

    Scenario: A composition of a character outside the Basic Multilingual Plane
      When the input method holds "ab𝄞" composing 2 to 4 with the cursor at 4
      Then the property "text" of "input" is "ab"
      And the property "preeditText" of "input" is "𝄞"

    Scenario: A word already typed is composed again
      When the input method holds "ab" composing 0 to 2 with the cursor at 2
      Then the property "text" of "input" is ""
      And the property "preeditText" of "input" is "ab"
      When the input method holds "ax" with the cursor at 2
      Then the property "text" of "input" is "ax"
      And the property "inputMethodComposing" of "input" is false

  Rule: The input method's selection is the editor's

    Scenario: The input method selects
      When the input method holds "ab" selecting from 0 to 2
      Then the property "selectedText" of "input" is "ab"
      And the property "text" of "input" is "ab"

    Scenario: The input method moves the cursor
      When the input method holds "ab" with the cursor at 1
      Then the property "cursorPosition" of "input" is 1
      And the property "text" of "input" is "ab"

  Rule: An input method is shown nothing of a field that hides its text

    Scenario: What is typed into a hidden field goes at its cursor
      When the property "focus" of "pin" is set to true
      And the input method holds "3" with the cursor at 1
      Then the property "text" of "pin" is "123"
