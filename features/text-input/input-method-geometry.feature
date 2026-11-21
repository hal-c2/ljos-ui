# Sources:
#   https://developer.android.com/reference/android/view/inputmethod/CursorAnchorInfo
#   https://developer.android.com/reference/android/view/inputmethod/InputConnection#requestCursorUpdates(int)
#   https://developer.android.com/reference/android/view/inputmethod/EditorBoundsInfo
#   https://github.com/hal-c2/ljos-ui/pull/646
# An input method that places its own windows by the text (a floating
# keyboard, handwriting, a candidate window) is told where the editor
# with focus is, where its cursor is and where each character it is
# composing stands, in the window's coordinates (Android's
# CursorAnchorInfo). A composed character outside the Basic Multilingual
# Plane counts two UTF-16 units, each told the character's bounds. The
# oracle's input platform has no input method, so these are Ljós's own.
@ljos-only
Feature: An input method is told where the text is
  The input method sees the editor, the cursor and the characters it
  composes where the window shows them, and is told anew when they move.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 300; height: 200
        Item {
          objectName: "box"
          x: 20; y: 40
          TextInput {
            objectName: "input"; focus: true
            x: 10; width: 200; height: 30
            font.family: "Liberation Sans"; font.pixelSize: 14
            text: "ab"
          }
        }
      }
      """

  Rule: The editor and its cursor are where the window shows them

    Scenario: The editor is where its parents put it
      Then the input method is told the editor is at 30, 40 sized 200 by 30

    Scenario: The cursor is after the text
      Then the input method is told the cursor is at 45.5625, 40 sized 1 by 16
      And the input method is told nothing is composed

    Scenario: A moved editor is told where it is now
      When the property "x" of "box" is set to 50
      Then the input method is told the editor is at 60, 40 sized 200 by 30
      And the input method is told the cursor is at 75.5625, 40 sized 1 by 16

  Rule: Each composed character is told where it stands

    Scenario: What is composed stands after the text, a character at a time
      When the input method holds "abni" composing 2 to 4 with the cursor at 4
      Then the input method is told the composed characters are at:
        | x        | y  | width    | height |
        | 45.5625  | 40 | 7.78125  | 16     |
        | 53.34375 | 40 | 3.109375 | 16     |
      And the input method is told the cursor is at 56.453125, 40 sized 1 by 16

    Scenario: A composition that ends leaves nothing composed
      When the input method holds "abni" composing 2 to 4 with the cursor at 4
      And the input method holds "abni" with the cursor at 4
      Then the input method is told nothing is composed
      And the input method is told the cursor is at 56.453125, 40 sized 1 by 16
