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
        objectName: "root"; width: 300; height: 200
        TextInput {
          objectName: "input"; focus: true; width: 200; text: "ab"
          font.family: "Liberation Sans"; font.pixelSize: 14
          property int edits: 0; onTextEdited: edits++
        }
        TextInput {
          objectName: "pin"; y: 50; width: 200; text: "12"
          echoMode: TextInput.Password
        }
        TextInput {
          objectName: "secret"; y: 100; width: 200; text: "xy"
          echoMode: TextInput.PasswordEchoOnEdit
        }
        TextInput {
          objectName: "guarded"; y: 150; width: 200; echoMode: TextInput.Password
          onCursorPositionChanged: if (cursorPosition === 1) text = ""
        }
        TextInput {
          objectName: "checked"; y: 170; width: 200; echoMode: TextInput.Password
          validator: RegularExpressionValidator { regularExpression: /[ab]*/ }
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

    Scenario: What the input method keeps typing is typed once
      When the property "focus" of "pin" is set to true
      And the input method holds "3" with the cursor at 1
      And the input method holds "34" with the cursor at 2
      Then the property "text" of "pin" is "1234"

    Scenario: What the input method takes back of its own typing is taken back
      When the property "focus" of "pin" is set to true
      And the input method holds "3" with the cursor at 1
      And the input method holds "34" with the cursor at 2
      And the input method holds "3" with the cursor at 1
      Then the property "text" of "pin" is "123"

    Scenario: The same character typed twice is typed twice
      When the property "focus" of "pin" is set to true
      And the input method holds "3" with the cursor at 1
      And the input method holds "33" with the cursor at 2
      Then the property "text" of "pin" is "1233"

    Scenario: A field changed by the application starts what the input method holds anew
      When the property "focus" of "pin" is set to true
      And the input method holds "3" with the cursor at 1
      And the property "text" of "pin" is set to ""
      And the input method holds "4" with the cursor at 1
      Then the property "text" of "pin" is "4"

    Scenario: The input method moves its cursor inside what it typed
      When the property "focus" of "pin" is set to true
      And the input method holds "abc" with the cursor at 3
      And the input method holds "abc" with the cursor at 1
      And the input method holds "axbc" with the cursor at 2
      Then the property "text" of "pin" is "12axbc"
      And the property "cursorPosition" of "pin" is 4

    Scenario: What the input method deletes after moving its cursor goes from there
      When the property "focus" of "pin" is set to true
      And the input method holds "abc" with the cursor at 3
      And the input method holds "abc" with the cursor at 1
      And the input method holds "bc" with the cursor at 0
      Then the property "text" of "pin" is "12bc"
      And the property "cursorPosition" of "pin" is 2

    Scenario: The input method selects inside what it typed
      When the property "focus" of "pin" is set to true
      And the input method holds "abc" with the cursor at 3
      And the input method holds "abc" selecting from 1 to 3
      Then the property "selectionStart" of "pin" is 3
      And the property "selectionEnd" of "pin" is 5

    Scenario: A field that clears on edit, focused again, holds none of what the input method typed before
      When the property "focus" of "secret" is set to true
      And the input method holds "abc" with the cursor at 3
      And the property "focus" of "input" is set to true
      And the property "focus" of "secret" is set to true
      And the input method holds "abcd" with the cursor at 4
      And the input method holds "abc" with the cursor at 3
      Then the property "text" of "secret" is "abc"

    Scenario: A field its handler clears as the input method moves the cursor holds none of what the method typed
      When the property "focus" of "guarded" is set to true
      And the input method holds "abc" with the cursor at 3
      And the input method holds "abc" with the cursor at 1
      And the input method holds "abxc" with the cursor at 3
      Then the property "text" of "guarded" is "abxc"

    Scenario: A hidden field whose validator refuses a change the same length holds none of it
      When the property "focus" of "checked" is set to true
      And the input method holds "ab" with the cursor at 2
      And the input method holds "ac" with the cursor at 2
      And the input method holds "acb" with the cursor at 3
      Then the property "text" of "checked" is "ab"

    Scenario: An input method that typed nothing yet leaves the field's selection
      When the property "focus" of "pin" is set to true
      And the user presses "Ctrl+A"
      And the input method holds "" with the cursor at 0
      Then the property "text" of "pin" is "12"
      And the property "selectionStart" of "pin" is 0
      And the property "selectionEnd" of "pin" is 2
