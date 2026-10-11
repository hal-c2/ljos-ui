# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#undo-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#redo-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#canUndo-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Undo and redo in a TextInput
  What the user typed in one go is undone in one go.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput { objectName: "input"; focus: true; width: 200 }
      }
      """

  Scenario: A run of typed characters is one step
    When the user types "hello world"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is ""
    And the property "canUndo" of "input" is false
    And the property "canRedo" of "input" is true

  Scenario: Redo brings the step back
    When the user types "hello"
    And the user presses "Ctrl+Z"
    And the user presses "Ctrl+Shift+Z"
    Then the property "text" of "input" is "hello"
    And the property "canRedo" of "input" is false
    And the property "cursorPosition" of "input" is 5

  Scenario: Moving the cursor ends a run
    When the user types "ab"
    And the user presses "Left"
    And the user types "c"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "ab"

  Scenario: Backspaces are a step of their own
    When the user types "ab"
    And the user presses "Backspace"
    And the user types "c"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "a"
    When the user presses "Ctrl+Z"
    Then the property "text" of "input" is "ab"
    When the user presses "Ctrl+Z"
    Then the property "text" of "input" is ""

  Scenario: Several backspaces in a row are one step
    When the user types "hello"
    And the user presses "Backspace"
    And the user presses "Backspace"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "hello"
    And the property "cursorPosition" of "input" is 5

  Scenario: Undo puts the cursor back where the edit began
    When the user types "hello"
    And the user presses "Home"
    And the user presses "Delete"
    And the user presses "Delete"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "hello"
    And the property "cursorPosition" of "input" is 0

  Scenario: Typing over a selection is undone together with what was typed
    When the property "text" of "input" is set to "hello"
    And the method "select" of "input" is called with 5, 3
    And the user types "xy"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "hello"
    And the property "cursorPosition" of "input" is 3

  Scenario: A new edit forgets what was undone
    When the user types "ab"
    And the user presses "Ctrl+Z"
    And the user types "c"
    Then the property "canRedo" of "input" is false

  Scenario: Assigning the text forgets the history
    When the user types "ab"
    And the property "text" of "input" is set to "xyz"
    Then the property "canUndo" of "input" is false

  Scenario: Changes made by the application in a row are one step
    When the method "insert" of "input" is called with 0, "abc"
    And the method "insert" of "input" is called with 3, "def"
    And the method "remove" of "input" is called with 0, 1
    And the method "undo" of "input" is called
    Then the property "text" of "input" is ""

  Scenario: Moving the cursor separates the application's changes
    When the method "insert" of "input" is called with 0, "abc"
    And the property "cursorPosition" of "input" is set to 0
    And the method "insert" of "input" is called with 3, "def"
    And the method "undo" of "input" is called
    Then the property "text" of "input" is "abc"

  Scenario: undo and redo are methods too
    When the user types "ab"
    And the method "undo" of "input" is called
    Then the property "text" of "input" is ""
    When the method "redo" of "input" is called
    Then the property "text" of "input" is "ab"
