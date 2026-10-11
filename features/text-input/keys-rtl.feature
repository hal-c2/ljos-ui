# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textinput.html
#   https://doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Moving in a right-to-left TextInput with the keyboard
  Left and Right follow what the eye sees: in a text that reads right to
  left, Left moves towards its end. Home and End stay the logical start and
  end of the line.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        width: 300; height: 200
        TextInput {
          objectName: "rtl"; focus: true
          width: 200
          text: "שלום עולם"
        }
      }
      """

  Scenario Outline: <key> from position <from> in a right-to-left text
    When the property "cursorPosition" of "rtl" is set to <from>
    And the user presses "<key>"
    Then the property "cursorPosition" of "rtl" is <to>

    Examples:
      | key        | from | to |
      | Left       | 0    | 1  |
      | Right      | 1    | 0  |
      | Left       | 9    | 9  |
      | Right      | 0    | 0  |
      | Home       | 4    | 0  |
      | End        | 4    | 9  |
      | Ctrl+Right | 9    | 5  |
      | Ctrl+Left  | 0    | 5  |
