# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#copy-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#cut-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#paste-method
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html#canPaste-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: The clipboard and a TextInput
  Cut, copy and paste, from the keyboard and from the application.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "input"; focus: true
          width: 200
          text: "hello brave world"
        }
      }
      """

  Scenario Outline: <key> copies the selection
    When the method "select" of "input" is called with 6, 11
    And the user presses "<key>"
    Then the clipboard holds "brave"
    And the property "text" of "input" is "hello brave world"

    Examples:
      | key         |
      | Ctrl+C      |
      | Ctrl+Insert |

  Scenario Outline: <key> cuts the selection
    When the method "select" of "input" is called with 5, 11
    And the user presses "<key>"
    Then the clipboard holds " brave"
    And the property "text" of "input" is "hello world"
    And the property "cursorPosition" of "input" is 5

    Examples:
      | key          |
      | Ctrl+X       |
      | Shift+Delete |

  Scenario Outline: <key> pastes at the cursor
    Given "new " was copied in another application
    When the property "cursorPosition" of "input" is set to 6
    And the user presses "<key>"
    Then the property "text" of "input" is "hello new brave world"
    And the property "cursorPosition" of "input" is 10

    Examples:
      | key          |
      | Ctrl+V       |
      | Shift+Insert |

  Scenario: Pasting replaces the selection
    Given "bold" was copied in another application
    When the method "select" of "input" is called with 6, 11
    And the user presses "Ctrl+V"
    Then the property "text" of "input" is "hello bold world"

  Scenario: A paste is one step to undo
    Given "new " was copied in another application
    When the user presses "Ctrl+V"
    And the user presses "Ctrl+Z"
    Then the property "text" of "input" is "hello brave world"

  Scenario: The methods do what the keys do
    When the method "select" of "input" is called with 0, 6
    And the method "cut" of "input" is called
    And the method "paste" of "input" is called
    And the method "paste" of "input" is called
    Then the property "text" of "input" is "hello hello brave world"
    And the clipboard holds "hello "

  Scenario: copy without a selection leaves the clipboard alone
    Given "kept" was copied in another application
    When the method "copy" of "input" is called
    Then the clipboard holds "kept"

  Scenario: canPaste says whether the clipboard holds text
    Given "x" was copied in another application
    Then the property "canPaste" of "input" is true

  Scenario: A read-only TextInput copies but does not cut or paste
    Given "x" was copied in another application
    When the property "readOnly" of "input" is set to true
    And the method "selectAll" of "input" is called
    And the user presses "Ctrl+V"
    And the user presses "Ctrl+X"
    Then the property "text" of "input" is "hello brave world"
    And the clipboard holds "x"
    And the property "canPaste" of "input" is false
